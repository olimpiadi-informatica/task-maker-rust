use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use failure::{format_err, Error};
use uuid::Uuid;

use task_maker_cache::{Cache, CacheResult};
use task_maker_dag::{
    CacheMode, Execution, ExecutionDAGData, ExecutionResult, ExecutionStatus, ExecutionUuid,
    FileUuid, WorkerUuid,
};
use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey};

use crate::executor::{
    ExecutionDAGWatchSet, ExecutorStatus, ExecutorWorkerStatus, WorkerCurrentJobStatus, WorkerJob,
};
use crate::worker_manager::WorkerManagerInMessage;

pub type ClientUuid = Uuid;

/// Information about a client of the scheduler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientInfo {
    /// Unique identifier of the client.
    pub uuid: ClientUuid,
    /// The name of the client.
    pub name: String,
}

/// Message coming in for the `Scheduler` from either an `Executor` or a `WorkerManager`.
pub(crate) enum SchedulerInMessage {
    /// A client asked to evaluate a DAG.
    EvaluateDAG {
        /// The information about the client issuing the request.
        client: ClientInfo,
        /// The DAG to evaluate.
        dag: ExecutionDAGData,
        /// The set of callbacks the client is interested in.
        callbacks: ExecutionDAGWatchSet,
    },
    /// A client has been disconnected, all the executions of that client should be removed and the
    /// involved workers stopped.
    ClientDisconnected {
        /// The identifier of the client.
        client: ClientUuid,
    },
    /// A new file of the DAG of a client is ready.
    FileReady {
        /// The identifier of the client that owns the file.
        client: ClientUuid,
        /// The identifier of the file that is now ready.
        uuid: FileUuid,
        /// The handle to the file in the store.
        handle: FileStoreHandle,
    },
    /// A worker completed its job.
    WorkerResult {
        /// The uuid of the worker that was doing the job.
        worker: WorkerUuid,
        /// The result of the execution.
        result: ExecutionResult,
        /// The outputs that the worker produced.
        outputs: HashMap<FileUuid, FileStoreHandle>,
    },
    /// A new worker is ready for executing some work.
    WorkerConnected {
        /// The uuid of the worker.
        uuid: WorkerUuid,
        /// The name of the worker.
        name: String,
    },
    /// A previously ready worker is not ready anymore.
    WorkerDisconnected {
        /// The uuid of the worker that has disconnected.
        uuid: WorkerUuid,
    },
    /// The executor is asking for the status of the scheduler.
    Status { client: ClientUuid },
    /// The executor is asking to exit.
    Exit,
}

/// Message that is sent from the `Scheduler` to the `Executor` informing a client that an event
/// occurred. All the identifiers of executions and files are relative to a client, which its
/// identifier is sent alongside this enum.
pub(crate) enum SchedulerExecutorMessageData {
    /// A watched execution started.
    ExecutionStarted {
        /// The uuid of the execution.
        execution: ExecutionUuid,
        /// The uuid of the worker on which the execution started.
        worker: WorkerUuid,
    },
    /// A watched execution completed.
    ExecutionDone {
        /// The uuid of the execution.
        execution: ExecutionUuid,
        /// The result of the execution.
        result: ExecutionResult,
    },
    /// A watched execution has been skipped because one of its dependencies failed.
    ExecutionSkipped {
        /// The uuid of the execution that has been skipped.
        execution: ExecutionUuid,
    },
    /// A watched file has been produced and its now ready.
    FileReady {
        /// The uuid of the produced file.
        file: FileUuid,
        /// An handle to the file inside the file store.
        handle: FileStoreHandle,
        /// Whether this file has been produced successfully or its execution failed doing so.
        successful: bool,
    },
    /// The evaluation has been completed.
    EvaluationDone,
    /// The status of the execution.
    Status { status: ExecutorStatus<Duration> },
}

/// The actual message sent from the Scheduler to an Executor. Since all the fields of the
/// enumeration would have got the client, it has been extracted here.
pub(crate) type SchedulerExecutorMessage = (ClientUuid, SchedulerExecutorMessageData);

/// The state of a connected worker.
#[derive(Debug)]
struct ConnectedWorker {
    /// The uuid of the worker.
    uuid: WorkerUuid,
    /// The name of the worker.
    name: String,
    /// The job the worker is currently working on, with the instant of the start.
    current_job: Option<(ClientUuid, ExecutionUuid, Instant)>,
}

/// The scheduling information about the DAG of a single client.
struct SchedulerClientData {
    /// The name of the client.
    name: String,
    /// The DAGs the scheduler is currently working on.
    dag: ExecutionDAGData,
    /// The set of callbacks the client is interested in.
    callbacks: ExecutionDAGWatchSet,
    /// The set of executions that depends on a file, this is a lookup table for when the files
    /// become ready.
    input_of: HashMap<FileUuid, HashSet<ExecutionUuid>>,
    /// The set of executions that are ready to be executed. Note that this is not the same as
    /// `Scheduler::ready_execs`, it's just a fast lookup for known if there is still something to
    /// do for this client.
    ready_execs: HashSet<ExecutionUuid>,
    /// The set of executions that are currently running in a worker.
    running_execs: HashSet<ExecutionUuid>,
    /// The list of tasks waiting for some dependencies, each with the list of missing files, when a
    /// task is ready it's removed from the map.
    missing_deps: HashMap<ExecutionUuid, HashSet<FileUuid>>,
    /// The list of known [`FileStoreHandle`](../task_maker_store/struct.FileStoreHandle.html)s.
    /// Storing them here prevents the `FileStore` from flushing them away.
    file_handles: HashMap<FileUuid, FileStoreHandle>,
}

impl SchedulerClientData {
    /// Make a new `SchedulerClientData` based on the DAG the client sent.
    fn new(
        name: String,
        dag: ExecutionDAGData,
        callbacks: ExecutionDAGWatchSet,
    ) -> SchedulerClientData {
        SchedulerClientData {
            name,
            dag,
            callbacks,
            input_of: HashMap::new(),
            ready_execs: HashSet::new(),
            running_execs: HashSet::new(),
            missing_deps: HashMap::new(),
            file_handles: HashMap::new(),
        }
    }

    /// True if the client has completed all the executions and there are no more ready nor running
    /// ones.
    fn is_done(&self) -> bool {
        self.ready_execs.is_empty() && self.running_execs.is_empty() && self.missing_deps.is_empty()
    }
}

/// A `Scheduler` is a service that is able to orchestrate the execution of the DAGs, sending the
/// jobs to the workers, listening for events and managing the cache of the executions.
///
/// The scheduler communicates with the Executor for knowing when a client connects, disconnects and
/// ask for the evaluation of a DAG, and sends messages to the clients via the Executor. It also
/// communicates with the WorkerManager for sending messages to the workers and known when a worker
/// connects or disconnects.
pub(crate) struct Scheduler {
    /// A reference to the local file store.
    file_store: Arc<FileStore>,
    /// The cache to use for the executions.
    cache: Cache,
    /// Receiver of the messages delivered to the scheduler.
    receiver: Receiver<SchedulerInMessage>,
    /// Sender of the messages to the Executor, aka the messages to the actual clients.
    executor: Sender<SchedulerExecutorMessage>,
    /// Sender of the messages to the WorkerManager, aka the messages to the workers.
    worker_manager: Sender<WorkerManagerInMessage>,

    /// The priority queue of the ready tasks, waiting for the workers.
    ///
    // TODO use something else for the priority
    ready_execs: BinaryHeap<(ClientUuid, ExecutionUuid)>,
    /// The data about the clients currently working.
    clients: HashMap<ClientUuid, SchedulerClientData>,

    /// The list of the workers that are either ready for some work or already working on a job.
    connected_workers: HashMap<WorkerUuid, ConnectedWorker>,
}

impl Scheduler {
    /// Make a new `Scheduler` based on the specified file store and cache. It will receive the
    /// messages using the provided channel and sends messages to the executor and worker manager
    /// with the specified channels.
    pub fn new(
        file_store: Arc<FileStore>,
        cache: Cache,
        receiver: Receiver<SchedulerInMessage>,
        executor: Sender<SchedulerExecutorMessage>,
        worker_manager: Sender<WorkerManagerInMessage>,
    ) -> Scheduler {
        Scheduler {
            file_store,
            cache,
            receiver,
            executor,
            worker_manager,

            ready_execs: BinaryHeap::new(),
            clients: HashMap::new(),

            connected_workers: HashMap::new(),
        }
    }

    /// Run the `Scheduler` listening for incoming messages and blocking util the scheduler is
    /// asked to exit. When the scheduler exits it will turn down the worker manager too.
    pub fn run(mut self) -> Result<(), Error> {
        while let Ok(message) = self.receiver.recv() {
            match message {
                SchedulerInMessage::Exit => {
                    debug!("Scheduler asked to exit");
                    break;
                }
                SchedulerInMessage::EvaluateDAG {
                    client,
                    dag,
                    callbacks,
                } => {
                    // build the scheduler structures, insert the client in the list of working
                    // clients and schedule all the already cached executions.
                    let mut client_data = SchedulerClientData::new(client.name, dag, callbacks);
                    for exec in client_data.dag.executions.values() {
                        let missing_dep = client_data.missing_deps.entry(exec.uuid).or_default();
                        for input in exec.dependencies() {
                            let entry = client_data.input_of.entry(input).or_default();
                            entry.insert(exec.uuid);
                            missing_dep.insert(input);
                        }
                    }
                    self.clients.insert(client.uuid, client_data);
                    // the client may have sent and empty DAG
                    self.check_completion(client.uuid)?;

                    self.schedule_cached()?;
                    self.assign_jobs()?;
                }
                SchedulerInMessage::FileReady {
                    client: client_uuid,
                    uuid,
                    handle,
                } => {
                    info!("Client sent a file {:?}", uuid);
                    if let Some(client) = self.clients.get_mut(&client_uuid) {
                        client.file_handles.insert(uuid, handle);
                        self.file_success(client_uuid, uuid)?;
                        self.check_completion(client_uuid)?;
                    } else {
                        warn!("Client is gone");
                    }
                }
                SchedulerInMessage::WorkerResult {
                    worker,
                    result,
                    outputs,
                } => {
                    let worker = match self.connected_workers.remove(&worker) {
                        Some(worker) => worker,
                        None => {
                            warn!("Unknown worker {} completed a job", worker);
                            continue;
                        }
                    };
                    let (client_uuid, exec_uuid) = match worker.current_job {
                        Some((client, exec, _)) => (client, exec),
                        None => {
                            warn!(
                                "Worker {} ({}) completed a job that wasn't doing",
                                worker.name, worker.uuid
                            );
                            continue;
                        }
                    };
                    let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                        client
                    } else {
                        warn!("Worker completed execution but client is gone");
                        continue;
                    };
                    let execution = client.dag.executions[&exec_uuid].clone();
                    info!("Worker {:?} completed execution {}", worker, execution.uuid);
                    client.running_execs.remove(&exec_uuid);
                    self.exec_completed(client_uuid, &execution, result, outputs)?;
                    self.assign_jobs()?;
                    self.check_completion(client_uuid)?;
                }
                SchedulerInMessage::WorkerConnected { uuid, name } => {
                    info!("Worker {} ({}) connected", name, uuid);
                    self.connected_workers.insert(
                        uuid,
                        ConnectedWorker {
                            uuid,
                            name,
                            current_job: None,
                        },
                    );
                    self.assign_jobs()?;
                }
                SchedulerInMessage::WorkerDisconnected { uuid } => {
                    info!("Worker {} disconnected", uuid);
                    if let Some(worker) = self.connected_workers.remove(&uuid) {
                        // reschedule the job if the worker failed
                        if let Some((client_uuid, job, _)) = worker.current_job {
                            let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                                client
                            } else {
                                warn!("Worker was doing something for a gone client");
                                continue;
                            };
                            self.ready_execs.push((client_uuid, job));
                            client.ready_execs.insert(job);
                            client.running_execs.remove(&job);
                        }
                    }
                }
                SchedulerInMessage::ClientDisconnected { client } => {
                    info!("Client {} disconnected", client);
                    if let Some(client) = self.clients.get(&client) {
                        if !client.is_done() {
                            warn!("The client's evaluation wasn't completed yet");
                        }
                    }
                    // TODO remove all the references to the client (ready_exec) and maybe tell the
                    //      worker to stop doing that execution
                    self.clients.remove(&client);
                }
                SchedulerInMessage::Status {
                    client: client_uuid,
                } => {
                    let mut ready_execs = 0;
                    let mut waiting_execs = 0;
                    for client in self.clients.values() {
                        ready_execs += client.ready_execs.len();
                        waiting_execs += client.missing_deps.len();
                    }
                    let status = ExecutorStatus {
                        connected_workers: self
                            .connected_workers
                            .values()
                            .map(|worker| ExecutorWorkerStatus {
                                uuid: worker.uuid,
                                name: worker.name.clone(),
                                current_job: worker.current_job.as_ref().and_then(
                                    |(client_uuid, exec_uuid, start)| {
                                        let client =
                                            if let Some(client) = self.clients.get(&client_uuid) {
                                                client
                                            } else {
                                                return None;
                                            };
                                        let exec = &client.dag.executions[exec_uuid];
                                        Some(WorkerCurrentJobStatus {
                                            job: exec.description.clone(),
                                            client: ClientInfo {
                                                uuid: *client_uuid,
                                                name: client.name.clone(),
                                            },
                                            duration: start.elapsed(),
                                        })
                                    },
                                ),
                            })
                            .collect(),
                        ready_execs,
                        waiting_execs,
                    };

                    self.executor
                        .send((client_uuid, SchedulerExecutorMessageData::Status { status }))?;
                }
            }
        }
        debug!("Scheduler exiting");
        self.worker_manager
            .send(WorkerManagerInMessage::Exit)
            .expect("Cannot tell the worker manager to exit");
        Ok(())
    }

    /// Check if the client has completed the evaluation, if so tell the client we are done.
    fn check_completion(&self, client_uuid: ClientUuid) -> Result<(), Error> {
        let client = if let Some(client) = self.clients.get(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        if client.is_done() {
            debug!("Computation completed for client: {}", client_uuid);
            self.executor
                .send((client_uuid, SchedulerExecutorMessageData::EvaluationDone))?;
        }
        Ok(())
    }

    /// Mark a file as failed, skipping all the executions that depends on it (even transitively).
    /// This will also send the file to the client, if needed.
    fn file_failed(&mut self, client_uuid: ClientUuid, file: FileUuid) -> Result<(), Error> {
        self.send_file(client_uuid, file, false)?;
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        if !client.input_of.contains_key(&file) {
            return Ok(());
        }
        let mut failed_files = Vec::new();
        for exec in client.input_of[&file].clone() {
            // do not skip the same execution twice
            if client.missing_deps.contains_key(&exec) {
                client.missing_deps.remove(&exec);
            } else {
                continue;
            }
            if client.callbacks.executions.contains(&exec) {
                self.executor.send((
                    client_uuid,
                    SchedulerExecutorMessageData::ExecutionSkipped { execution: exec },
                ))?;
            }
            let exec = &client.dag.executions[&exec];
            for output in exec.outputs() {
                failed_files.push((client_uuid, output));
            }
        }
        for (client_uuid, output) in failed_files {
            self.file_failed(client_uuid, output)?;
        }
        Ok(())
    }

    /// Mark a file as successful and schedule all the executions that become ready.
    /// This will also send the file to the client, if needed.
    fn file_success(&mut self, client_uuid: ClientUuid, file: FileUuid) -> Result<(), Error> {
        self.send_file(client_uuid, file, true)?;
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        if !client.input_of.contains_key(&file) {
            return Ok(());
        }
        for exec in &client.input_of[&file] {
            if let Some(files) = client.missing_deps.get_mut(exec) {
                files.remove(&file);
                if files.is_empty() {
                    client.missing_deps.remove(exec);
                    self.ready_execs.push((client_uuid, *exec));
                    client.ready_execs.insert(*exec);
                }
            }
        }
        self.schedule_cached()?;
        self.assign_jobs()?;
        Ok(())
    }

    /// Send a file to the client if its uuid is included in the callbacks.
    fn send_file(
        &mut self,
        client_uuid: ClientUuid,
        file: FileUuid,
        status: bool,
    ) -> Result<(), Error> {
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        if !client.callbacks.files.contains(&file) {
            return Ok(());
        }
        if !client.file_handles.contains_key(&file) {
            return Ok(());
        }
        self.executor.send((
            client_uuid,
            SchedulerExecutorMessageData::FileReady {
                file,
                handle: client.file_handles[&file].clone(),
                successful: status,
            },
        ))?;
        Ok(())
    }

    /// Mark an execution as completed, sending the notification to the client and marking all the
    /// produced files as done. Add the execution to the cache and schedule all the new executions
    /// that become ready.
    fn exec_completed(
        &mut self,
        client_uuid: ClientUuid,
        execution: &Execution,
        result: ExecutionResult,
        outputs: HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        if client.callbacks.executions.contains(&execution.uuid) {
            self.executor.send((
                client_uuid,
                SchedulerExecutorMessageData::ExecutionDone {
                    execution: execution.uuid,
                    result: result.clone(),
                },
            ))?;
        }
        for (uuid, handle) in outputs.iter() {
            client.file_handles.insert(*uuid, handle.clone());
        }
        let successful = ExecutionStatus::Success == result.status;
        match &result.status {
            ExecutionStatus::InternalError(_) => {} // do not cache internal errors
            _ => self.cache_execution(client_uuid, &execution, outputs, result),
        }
        if successful {
            for output in execution.outputs() {
                self.file_success(client_uuid, output)?;
            }
        } else {
            for output in execution.outputs() {
                self.file_failed(client_uuid, output)?;
            }
        }
        self.schedule_cached()?;
        Ok(())
    }

    /// Store an execution in the cache.
    fn cache_execution(
        &mut self,
        client_uuid: ClientUuid,
        execution: &Execution,
        outputs: HashMap<FileUuid, FileStoreHandle>,
        result: ExecutionResult,
    ) {
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return;
        };
        let mut file_keys: HashMap<FileUuid, FileStoreKey> = execution
            .dependencies()
            .iter()
            .map(|f| (*f, client.file_handles[f].key().clone()))
            .collect();
        for output in execution.outputs() {
            file_keys.insert(output, outputs[&output].key().clone());
        }
        self.cache.insert(execution, &client.file_handles, result);
    }

    /// Look at all the ready executions and mark as completed all the ones that are inside the
    /// cache.
    fn schedule_cached(&mut self) -> Result<(), Error> {
        let mut not_cached = BinaryHeap::new();
        let mut cached = Vec::new();

        for (client_uuid, exec) in self.ready_execs.iter() {
            let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                client
            } else {
                // client is gone, dont worry to much about it
                continue;
            };
            let dag = &client.dag;
            let cache_mode = &dag.config.cache_mode;
            // disable the cache for the execution
            if let CacheMode::Nothing = cache_mode {
                not_cached.push((*client_uuid, *exec));
                continue;
            }
            let exec = dag.executions[exec].clone();
            if !Scheduler::is_cacheable(&exec, &cache_mode) {
                not_cached.push((*client_uuid, exec.uuid));
                continue;
            }
            let result = self
                .cache
                .get(&exec, &client.file_handles, self.file_store.as_ref());
            match result {
                CacheResult::Hit { result, outputs } => {
                    info!("Execution {} is a cache hit!", exec.uuid);
                    client.ready_execs.remove(&exec.uuid);
                    cached.push((*client_uuid, exec, result, outputs));
                }
                CacheResult::Miss => {
                    not_cached.push((*client_uuid, exec.uuid));
                }
            }
        }

        self.ready_execs = not_cached;
        for (client, exec, result, outputs) in cached.into_iter() {
            self.exec_completed(client, &exec, result, outputs)?;
        }

        Ok(())
    }

    /// Whether an execution is eligible to be fetch from the cache.
    fn is_cacheable(execution: &Execution, cache_mode: &CacheMode) -> bool {
        if let (CacheMode::Except(set), Some(tag)) = (cache_mode, execution.tag.as_ref()) {
            if set.contains(tag) {
                return false;
            }
        }
        true
    }

    /// Give to each free worker a job from the ready executions.
    fn assign_jobs(&mut self) -> Result<(), Error> {
        for (worker_uuid, worker) in self.connected_workers.iter_mut() {
            if worker.current_job.is_some() {
                continue;
            }
            let (client_uuid, exec) = match self.ready_execs.pop() {
                Some(exec) => exec,
                None => break,
            };
            trace!("Assigning {} to worker {}", exec, worker_uuid);
            worker.current_job = Some((client_uuid, exec, Instant::now()));
            let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                client
            } else {
                // client is gone, dont worry to much about it
                continue;
            };
            client.ready_execs.remove(&exec);
            client.running_execs.insert(exec);
            let execution = client.dag.executions[&exec].clone();
            let dep_keys = execution
                .dependencies()
                .iter()
                .map(|k| {
                    (
                        *k,
                        client
                            .file_handles
                            .get(k)
                            .unwrap_or_else(|| panic!("Unknown file key of {}", k))
                            .key()
                            .clone(),
                    )
                })
                .collect();
            let job = WorkerJob {
                execution,
                dep_keys,
            };
            self.worker_manager
                .send(WorkerManagerInMessage::WorkerJob {
                    worker: *worker_uuid,
                    job,
                })
                .map_err(|e| format_err!("Failed to send job to worker: {:?}", e))?;
            if client.callbacks.executions.contains(&exec) {
                self.executor.send((
                    client_uuid,
                    SchedulerExecutorMessageData::ExecutionStarted {
                        execution: exec,
                        worker: *worker_uuid,
                    },
                ))?;
            }
        }
        Ok(())
    }
}
