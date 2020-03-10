use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use chashmap::CHashMap;
use failure::{format_err, Error};
use serde::{Deserialize, Serialize};

use task_maker_cache::Cache;
use task_maker_dag::{
    Execution, ExecutionGroup, ExecutionUuid, FileUuid, ProvidedFile, WorkerUuid,
};
use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey};

use crate::check_dag::check_dag;
use crate::proto::{
    ChannelFileIterator, ChannelFileSender, ExecutorClientMessage, ExecutorServerMessage,
};
use crate::scheduler::{
    ClientInfo, ClientUuid, Scheduler, SchedulerExecutorMessage, SchedulerExecutorMessageData,
    SchedulerInMessage,
};
use crate::worker_manager::{WorkerManager, WorkerManagerInMessage};
use crate::{ChannelReceiver, ChannelSender, WorkerConn};
use failure::_core::time::Duration;
use std::time::SystemTime;

/// List of the _interesting_ files and executions, only the callbacks listed here will be called by
/// the server. Every other callback is not sent to the client for performance reasons.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionDAGWatchSet {
    /// Set of the handles of the executions that have at least a callback bound.
    pub executions: HashSet<ExecutionUuid>,
    /// Set of the handles of the files that have at least a callback bound.
    pub files: HashSet<FileUuid>,
    /// Set of the handles of the files that should be sent to the client as soon as possible. The
    /// others will be sent at the end of the evaluation. Note that sending big files during the
    /// evaluation can cause performance degradations.
    pub urgent_files: HashSet<FileUuid>,
}

/// A job that is sent to a worker, this should include all the information the worker needs to
/// start the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerJob {
    /// What the worker should do.
    pub group: ExecutionGroup,
    /// The `FileStoreKey`s the worker has to know to start the evaluation.
    pub dep_keys: HashMap<FileUuid, FileStoreKey>,
}

/// Information about the job the worker is currently doing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerCurrentJobStatus<T> {
    /// The name of the job the worker is currently doing.
    pub job: String,
    /// UUID and name of the client who owns the job.
    pub client: ClientInfo,
    /// Since when the job started.
    pub duration: T,
}

impl WorkerCurrentJobStatus<Duration> {
    /// Convert a status based on a `Duration` (the one sent in the network) to a status based on
    /// the system time.
    pub fn into_system_time(self) -> WorkerCurrentJobStatus<SystemTime> {
        WorkerCurrentJobStatus {
            job: self.job,
            client: self.client,
            duration: SystemTime::now() - self.duration,
        }
    }
}

/// Status of a worker of an `Executor`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutorWorkerStatus<T> {
    /// UUID of the worker.
    pub uuid: WorkerUuid,
    /// Name of the worker.
    pub name: String,
    /// What the worker is currently working on.
    pub current_job: Option<WorkerCurrentJobStatus<T>>,
}

/// The current status of the `Executor`, this is sent to the user when the server status is asked.
///
/// The type parameter `T` is either `SystemTime` for local usage or `Duration` for serialization.
/// Unfortunately since `Instant` is not serializable by design, it cannot be used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutorStatus<T> {
    /// List of the connected workers with their uuid, name and if they have some work.
    pub connected_workers: Vec<ExecutorWorkerStatus<T>>,
    /// Number of executions waiting for workers.
    pub ready_execs: usize,
    /// Number of executions waiting for dependencies.
    pub waiting_execs: usize,
}

/// Message telling the executor that a new client connected or a new worker connected. The handling
/// of the new peer is done by this executor.
pub enum ExecutorInMessage {
    /// A new client has connected, the executor starts listening for the messages and will directly
    /// interact with it.
    ClientConnected {
        /// The information about the new client.
        client: ClientInfo,
        /// A channel for sending messages to the client.
        sender: ChannelSender<ExecutorServerMessage>,
        /// A channel for received the messages from the client.
        receiver: ChannelReceiver<ExecutorClientMessage>,
    },
    /// A new worker has connected, the executor starts listening for the messages and will directly
    /// interact with it.
    WorkerConnected {
        /// The information and connection details of the worker.
        worker: WorkerConn,
    },
}

/// The `Executor` is the main component of the server, this will listen for client and worker
/// connections, handing them by listening to their messages. The clients will send the DAGs to the
/// `Executor`, which will use its scheduler for executing the jobs. The workers will be attached
/// to the `WorkerManager` which is being used by the `Scheduler` for assigning the jobs.
pub(crate) struct Executor {
    /// The file store used by the Scheduler and the WorkerManager for the local keeping of the
    /// files.
    file_store: Arc<FileStore>,
    /// The `Cache` the Scheduler will use.
    cache: Cache,
    /// The receiver of the messages for the `Executor`. The actual `LocalExecutor`/`RemoteExecutor`
    /// use this channel for the communication.
    receiver: Receiver<ExecutorInMessage>,
    /// Whether this executor is running for more than a single client (aka not locally). When this
    /// flag is set to false, after the first client is done the Scheduler, the WorkerManager and
    /// this Executor will exit.
    long_running: bool,
}

impl Executor {
    /// Create a new `Executor` using the specified `FileStore` for the Scheduler and WorkerManager,
    /// the receiver for communicating with this Executor and if it should be "long running".
    /// When this flag is set to false, after the first client is done the Scheduler, the
    /// WorkerManager and this Executor will exit.
    pub fn new(
        file_store: Arc<FileStore>,
        cache: Cache,
        receiver: Receiver<ExecutorInMessage>,
        long_running: bool,
    ) -> Executor {
        Executor {
            file_store,
            cache,
            receiver,
            long_running,
        }
    }

    /// Run the `Executor`, listening for client and worker connections. This will block until the
    /// first client is done (if `long_running` is false) or until the scheduler is stopped.
    pub fn run(self) -> Result<(), Error> {
        let (scheduler_tx, scheduler_rx) = channel();
        let (worker_manager_tx, worker_manager_rx) = channel();
        let (sched_executor_tx, sched_executor_rx) = channel();

        let clients = Arc::new(CHashMap::new());

        let scheduler = Scheduler::new(
            self.file_store.clone(),
            self.cache,
            scheduler_rx,
            sched_executor_tx,
            worker_manager_tx.clone(),
        );
        let worker_manager = WorkerManager::new(
            self.file_store.clone(),
            scheduler_tx.clone(),
            worker_manager_tx.clone(),
            worker_manager_rx,
        );
        let scheduler_thread = thread::Builder::new()
            .name("Scheduler thread".to_string())
            .spawn(move || scheduler.run().expect("Scheduler failed"))
            .expect("Failed to spawn scheduler");
        let worker_manager_thread = thread::Builder::new()
            .name("Worker Manager thread".to_string())
            .spawn(move || worker_manager.run().expect("Worker manager failed"))
            .expect("Failed to spawn worker manager");
        let clients2 = clients.clone();
        let scheduler_binder_thread = thread::Builder::new()
            .name("Scheduler binder".to_string())
            .spawn(move || {
                Executor::handle_scheduler_messages(sched_executor_rx, clients2)
                    .expect("Scheduler binder failed")
            })
            .expect("Failed to spawn scheduler binder");

        while let Ok(message) = self.receiver.recv() {
            match message {
                ExecutorInMessage::ClientConnected {
                    client,
                    sender,
                    receiver,
                } => {
                    clients.insert(client.uuid, sender.clone());
                    let scheduler = scheduler_tx.clone();
                    let file_store = self.file_store.clone();
                    let long_running = self.long_running;
                    // handle the new client in a new thread called "Client Manager"
                    thread::Builder::new()
                        .name(format!(
                            "Client manager for {} ({})",
                            client.name, client.uuid
                        ))
                        .spawn(move || {
                            Executor::handle_client_messages(
                                file_store,
                                client,
                                sender,
                                receiver,
                                scheduler.clone(),
                            )
                            .expect("Client manager failed");
                            // if not in long running mode, the first client should tear down the
                            // executor. To do so it's just required to tell the scheduler to exit,
                            // it will bring down the WorkerManager and all should exit.
                            if !long_running {
                                scheduler
                                    .send(SchedulerInMessage::Exit)
                                    .expect("Cannot stop the scheduler");
                            }
                        })
                        .expect("Failed to spawn client manager");
                }
                ExecutorInMessage::WorkerConnected { worker } => {
                    worker_manager_tx
                        .send(WorkerManagerInMessage::WorkerConnected { worker })
                        .expect("WorkerManager died");
                }
            }
        }
        debug!("Executor no longer waits for clients/workers");

        scheduler_thread.join().unwrap();
        worker_manager_thread.join().unwrap();
        scheduler_binder_thread.join().unwrap();
        Ok(())
    }

    /// Handle the messages from the scheduler, sending the notifications to the client involved.
    fn handle_scheduler_messages(
        receiver: Receiver<SchedulerExecutorMessage>,
        clients: Arc<CHashMap<ClientUuid, ChannelSender<ExecutorServerMessage>>>,
    ) -> Result<(), Error> {
        let mut ready_files: HashMap<ClientUuid, Vec<(FileUuid, FileStoreHandle, bool)>> =
            HashMap::new();
        while let Ok((client_uuid, message)) = receiver.recv() {
            let client = if let Some(client) = clients.get(&client_uuid) {
                client
            } else {
                // ignore messages for a disconnected client
                continue;
            };
            let message = match message {
                SchedulerExecutorMessageData::ExecutionStarted { execution, worker } => {
                    ExecutorServerMessage::NotifyStart(execution, worker)
                }
                SchedulerExecutorMessageData::ExecutionSkipped { execution } => {
                    ExecutorServerMessage::NotifySkip(execution)
                }
                SchedulerExecutorMessageData::ExecutionDone { execution, result } => {
                    ExecutorServerMessage::NotifyDone(execution, result)
                }
                SchedulerExecutorMessageData::FileReady {
                    file,
                    handle,
                    successful,
                    urgent,
                } => {
                    if urgent {
                        if let Err(e) =
                            client.send(ExecutorServerMessage::ProvideFile(file, successful))
                        {
                            warn!("Failed to send urgent file: {:?}", e);
                        } else if let Err(e) = ChannelFileSender::send(handle.path(), &client) {
                            warn!("Failed to send urgent file content: {:?}", e);
                        }
                        continue;
                    } else {
                        ready_files
                            .entry(client_uuid)
                            .or_default()
                            .push((file, handle, successful));
                        continue;
                    }
                }
                SchedulerExecutorMessageData::Status { status } => {
                    ExecutorServerMessage::Status(status)
                }
                SchedulerExecutorMessageData::EvaluationDone => {
                    let files = ready_files
                        .remove(&client_uuid)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(f, h, s)| (f, h.key().clone(), s))
                        .collect();
                    ExecutorServerMessage::Done(files)
                }
            };
            if let Err(e) = client.send(message) {
                warn!("Failed to send message to the client: {:?}", e);
            }
        }
        debug!("Scheduler binder exiting");
        Ok(())
    }

    /// Handle the messages from a client.
    fn handle_client_messages(
        file_store: Arc<FileStore>,
        client: ClientInfo,
        sender: ChannelSender<ExecutorServerMessage>,
        receiver: ChannelReceiver<ExecutorClientMessage>,
        scheduler: Sender<SchedulerInMessage>,
    ) -> Result<(), Error> {
        while let Ok(message) = receiver.recv() {
            match message {
                ExecutorClientMessage::Evaluate { dag, callbacks } => {
                    if let Err(e) = check_dag(&dag, &callbacks) {
                        warn!("Invalid DAG: {:?}", e);
                        sender.send(ExecutorServerMessage::Error(e.to_string()))?;
                        break;
                    } else {
                        trace!("DAG looks valid!");
                    }
                    // for each file marked as provided check if a local copy is present, otherwise
                    // ask the client to send it.
                    let mut ready_files = Vec::new();
                    for (uuid, file) in dag.provided_files.iter() {
                        let key = match file {
                            ProvidedFile::Content { key, .. } => key,
                            ProvidedFile::LocalFile { key, .. } => key,
                        };
                        let handle = file_store.get(&key);
                        if let Some(handle) = handle {
                            ready_files.push((*uuid, handle));
                        } else {
                            sender.send(ExecutorServerMessage::AskFile(*uuid))?;
                        }
                    }
                    // tell the scheduler that a new DAG is ready to be executed.
                    scheduler
                        .send(SchedulerInMessage::EvaluateDAG {
                            client: client.clone(),
                            dag,
                            callbacks,
                        })
                        .map_err(|e| format_err!("Failed to send message to scheduler: {:?}", e))?;
                    // tell the scheduler the files that are already locally ready. The others will
                    // be ready when the client will send them.
                    for (uuid, handle) in ready_files.into_iter() {
                        scheduler
                            .send(SchedulerInMessage::FileReady {
                                client: client.uuid,
                                uuid,
                                handle,
                            })
                            .map_err(|e| {
                                format_err!("Failed to send message to scheduler: {:?}", e)
                            })?;
                    }
                }
                ExecutorClientMessage::ProvideFile(uuid, key) => {
                    info!("Client provided file {}", uuid);
                    // the client provided a file that was not present locally, store it and tell
                    // the scheduler that it's now ready.
                    let handle = file_store.store(&key, ChannelFileIterator::new(&receiver))?;
                    scheduler
                        .send(SchedulerInMessage::FileReady {
                            client: client.uuid,
                            uuid,
                            handle,
                        })
                        .map_err(|e| format_err!("Failed to send message to scheduler: {:?}", e))?;
                }
                ExecutorClientMessage::AskFile(uuid, key, success) => {
                    info!("Client asking file {:?}", key);
                    // the client wants to know a file that was produced by the computation, send it
                    // if it exists.
                    if let Some(handle) = file_store.get(&key) {
                        sender.send(ExecutorServerMessage::ProvideFile(uuid, success))?;
                        ChannelFileSender::send(handle.path(), &sender)?;
                    } else {
                        sender.send(ExecutorServerMessage::Error(format!(
                            "Unknown file {:?}",
                            key
                        )))?;
                    }
                }
                ExecutorClientMessage::Status => {
                    info!("Client asking for the status");
                    // this may fail is the scheduler is gone
                    let _ = scheduler.send(SchedulerInMessage::Status {
                        client: client.uuid,
                    });
                }
                ExecutorClientMessage::Stop => {
                    info!("Client asking to stop");
                    break;
                }
            }
        }
        scheduler.send(SchedulerInMessage::ClientDisconnected {
            client: client.uuid,
        })?;
        Ok(())
    }
}
