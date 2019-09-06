use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use failure::{format_err, Error};
use serde::{Deserialize, Serialize};
use task_maker_dag::*;
use task_maker_store::*;

use crate::proto::*;
use crate::*;
use task_maker_cache::Cache;

/// List of the _interesting_ files and executions, only the callbacks listed here will be called by
/// the server. Every other callback is not sent to the client for performance reasons.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ExecutionDAGWatchSet {
    /// Set of the handles of the executions that have at least a callback bound.
    pub executions: HashSet<ExecutionUuid>,
    /// Set of the handles of the files that have at least a callback bound.
    pub files: HashSet<FileUuid>,
}

/// A job that is sent to a worker, this should include all the information the worker needs to
/// start the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerJob {
    /// What the worker should do.
    pub execution: Execution,
    /// The `FileStoreKey`s the worker has to know to start the evaluation.
    pub dep_keys: HashMap<FileUuid, FileStoreKey>,
}

/// Status of a worker of an `Executor`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutorWorkerStatus<T> {
    /// UUID of the worker.
    pub uuid: WorkerUuid,
    /// Name of the worker.
    pub name: String,
    /// What the worker is currently working on: the description of the execution and the duration
    /// of that.
    pub current_job: Option<(String, T)>,
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

/// The `Executor` is the main component of the server, this will receive the DAG to evaluate and
/// will schedule the tasks to the workers, sending to the client the responses.
pub(crate) struct Executor {
    /// A reference to the `FileStore`. Will be used for the `Scheduler` and for storing the files
    /// from/to the client.
    file_store: Arc<FileStore>,
    /// A channel for communicating to the `Scheduler`.
    pub(crate) scheduler_tx: Sender<SchedulerInMessage>,
    /// The receiving part of the `Scheduler`. Will be consumed when the `Scheduler` is
    /// instantiated.
    scheduler_rx: Option<Receiver<SchedulerInMessage>>,
}

impl Executor {
    /// Make a new `Executor` based on the specified
    /// [`FileStore`](../task_maker_store/struct.FileStore.html).
    pub fn new(file_store: Arc<FileStore>) -> Executor {
        let (sched_tx, sched_rx) = channel();
        Executor {
            file_store,
            scheduler_tx: sched_tx,
            scheduler_rx: Some(sched_rx),
        }
    }

    /// Starts the `Executor` for a client, this will block and will manage the communication with
    /// the client.
    ///
    /// * `sender` - A channel that sends messages to the client.
    /// * `receiver` - A channel that receives messages from the client.
    pub fn evaluate(
        mut self,
        client_tx: ChannelSender,
        client_rx: ChannelReceiver,
        cache: Cache,
    ) -> Result<(), Error> {
        let (sched_binder_tx, sched_binder_rx) = channel();
        let sched_binder_client = client_tx.clone();
        let join_scheduler_binder = std::thread::Builder::new()
            .name("Scheduler binder".into())
            .spawn(move || {
                Executor::scheduler_thread(sched_binder_rx, sched_binder_client).unwrap()
            })
            .expect("Failed to spawn scheduler binder thread");

        let scheduler = Scheduler::new(cache, self.file_store.clone(), sched_binder_tx);
        let sched_rx = self
            .scheduler_rx
            .take()
            .ok_or_else(|| format_err!("Evaluate called more than once"))?;
        let join_scheduler = std::thread::Builder::new()
            .name("Scheduler".into())
            .spawn(move || {
                scheduler.work(sched_rx).unwrap();
            })
            .expect("Failed to spawn the scheduler");

        loop {
            let message = deserialize_from::<ExecutorClientMessage>(&client_rx);
            match message {
                Ok(ExecutorClientMessage::Evaluate { dag, callbacks }) => {
                    if let Err(e) = check_dag(&dag, &callbacks) {
                        warn!("Invalid DAG: {:?}", e);
                        serialize_into(&ExecutorServerMessage::Error(e.to_string()), &client_tx)?;
                        break;
                    } else {
                        trace!("DAG looks valid!");
                    }
                    let mut ready_files = Vec::new();
                    for (uuid, file) in dag.provided_files.iter() {
                        let key = match file {
                            ProvidedFile::Content { key, .. } => key,
                            ProvidedFile::LocalFile { key, .. } => key,
                        };
                        let handle = self.file_store.get(&key);
                        if let Some(handle) = handle {
                            ready_files.push((*uuid, handle));
                        } else {
                            serialize_into(&ExecutorServerMessage::AskFile(*uuid), &client_tx)?;
                        }
                    }
                    self.scheduler_tx
                        .send(SchedulerInMessage::DAG { dag, callbacks })
                        .map_err(|e| format_err!("Failed to send message to scheduler: {:?}", e))?;
                    for (uuid, handle) in ready_files.into_iter() {
                        self.scheduler_tx
                            .send(SchedulerInMessage::FileReady { uuid, handle })
                            .map_err(|e| {
                                format_err!("Failed to send message to scheduler: {:?}", e)
                            })?;
                    }
                }
                Ok(ExecutorClientMessage::ProvideFile(uuid, key)) => {
                    info!("Client provided file {}", uuid);
                    let handle = self
                        .file_store
                        .store(&key, ChannelFileIterator::new(&client_rx))?;
                    self.scheduler_tx
                        .send(SchedulerInMessage::FileReady { uuid, handle })
                        .map_err(|e| format_err!("Failed to send message to scheduler: {:?}", e))?;
                }
                Ok(ExecutorClientMessage::Status) => {
                    info!("Client asking for the status");
                    // this may fail is the scheduler is gone
                    let _ = self.scheduler_tx.send(SchedulerInMessage::Status);
                }
                Ok(ExecutorClientMessage::Stop) => {
                    info!("Client asking to stop");
                    unimplemented!();
                }
                Err(_) => {
                    // the receiver has been dropped
                    break;
                }
            }
        }
        let _ = self.scheduler_tx.send(SchedulerInMessage::Exit);
        join_scheduler.join().expect("Scheduler thread panicked");
        join_scheduler_binder
            .join()
            .expect("Scheduler binder thread panicked");
        Ok(())
    }

    /// Thread that will receive messages from the scheduler and will forward them to the client,
    /// eventually blocking reading files.
    ///
    /// This function will block until the `Scheduler` drops its sender.
    fn scheduler_thread(
        receiver: Receiver<SchedulerOutMessage>,
        client_tx: ChannelSender,
    ) -> Result<(), Error> {
        loop {
            let message = receiver.recv();
            match message {
                Ok(SchedulerOutMessage::ExecutionStarted(exec, worker)) => {
                    serialize_into(
                        &ExecutorServerMessage::NotifyStart(exec, worker),
                        &client_tx,
                    )?;
                }
                Ok(SchedulerOutMessage::ExecutionSkipped(exec)) => {
                    serialize_into(&ExecutorServerMessage::NotifySkip(exec), &client_tx)?;
                }
                Ok(SchedulerOutMessage::ExecutionDone(exec, result)) => {
                    serialize_into(&ExecutorServerMessage::NotifyDone(exec, result), &client_tx)?;
                }
                Ok(SchedulerOutMessage::FileReady(uuid, handle, success)) => {
                    serialize_into(
                        &ExecutorServerMessage::ProvideFile(uuid, success),
                        &client_tx,
                    )?;
                    ChannelFileSender::send(handle.path(), &client_tx)?;
                }
                Ok(SchedulerOutMessage::Status(status)) => {
                    serialize_into(&ExecutorServerMessage::Status(status), &client_tx)?;
                }
                Err(_) => {
                    // Scheduler is gone.
                    break;
                }
            }
        }
        // at this point the scheduler is gone, the evaluation has to end.
        let _ = serialize_into(&ExecutorServerMessage::Done, &client_tx);
        Ok(())
    }
}
