use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use failure::{format_err, Error};
use uuid::Uuid;

use task_maker_cache::{Cache, CacheResult};
use task_maker_dag::{
    CacheMode, Execution, ExecutionDAGData, ExecutionGroup, ExecutionGroupUuid, ExecutionResult,
    ExecutionStatus, ExecutionUuid, FileUuid, Priority, WorkerUuid,
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
        dag: Box<ExecutionDAGData>,
        /// The set of callbacks the client is interested in.
        callbacks: Box<ExecutionDAGWatchSet>,
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
        /// This file is urgent, it should be sent to the client ASAP.
        urgent: bool,
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
    current_job: Option<(ClientUuid, ExecutionGroupUuid, Instant)>,
}

/// The scheduling information about the DAG of a single client.
#[derive(Debug)]
struct SchedulerClientData {
    /// The name of the client.
    name: String,
    /// The DAGs the scheduler is currently working on.
    dag: ExecutionDAGData,
    /// The set of callbacks the client is interested in.
    callbacks: ExecutionDAGWatchSet,
    /// The set of executions that depends on a file, this is a lookup table for when the files
    /// become ready.
    input_of: HashMap<FileUuid, HashSet<ExecutionGroupUuid>>,
    /// The set of executions that are ready to be executed. Note that this is not the same as
    /// `Scheduler::ready_execs`, it's just a fast lookup for known if there is still something to
    /// do for this client.
    ready_groups: HashSet<ExecutionGroupUuid>,
    /// The set of executions that are currently running in a worker.
    running_groups: HashSet<ExecutionGroupUuid>,
    /// The list of tasks waiting for some dependencies, each with the list of missing files, when a
    /// task is ready it's removed from the map.
    missing_deps: HashMap<ExecutionGroupUuid, HashSet<FileUuid>>,
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
            ready_groups: HashSet::new(),
            running_groups: HashSet::new(),
            missing_deps: HashMap::new(),
            file_handles: HashMap::new(),
        }
    }

    /// True if the client has completed all the executions and there are no more ready nor running
    /// ones.
    fn is_done(&self) -> bool {
        self.ready_groups.is_empty()
            && self.running_groups.is_empty()
            && self.missing_deps.is_empty()
    }
}

/// A `Scheduler` is a service that is able to orchestrate the execution of the DAGs, sending the
/// jobs to the workers, listening for events and managing the cache of the executions.
///
/// The scheduler communicates with the Executor for knowing when a client connects, disconnects and
/// ask for the evaluation of a DAG, and sends messages to the clients via the Executor. It also
/// communicates with the WorkerManager for sending messages to the workers and known when a worker
/// connects or disconnects.
#[derive(Debug)]
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
    ready_execs: BinaryHeap<(Priority, ExecutionGroupUuid, ClientUuid)>,
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
                    self.handle_evaluate_dag(client, *dag, *callbacks)?;
                }
                SchedulerInMessage::FileReady {
                    client,
                    uuid,
                    handle,
                } => {
                    self.handle_file_ready(client, uuid, handle)?;
                }
                SchedulerInMessage::WorkerResult {
                    worker,
                    result,
                    outputs,
                } => {
                    self.handle_worker_result(worker, result, outputs)?;
                }
                SchedulerInMessage::WorkerConnected { uuid, name } => {
                    self.handle_worker_connected(uuid, name)?;
                }
                SchedulerInMessage::WorkerDisconnected { uuid } => {
                    self.handle_worker_disconnected(uuid)?;
                }
                SchedulerInMessage::ClientDisconnected { client } => {
                    self.handle_client_disconnected(client)?;
                }
                SchedulerInMessage::Status { client } => {
                    self.handle_status_request(client)?;
                }
            }
        }
        debug!("Scheduler exiting");
        self.worker_manager
            .send(WorkerManagerInMessage::Exit)
            .expect("Cannot tell the worker manager to exit");
        Ok(())
    }

    /// Handle the client request to evaluate a DAG.
    fn handle_evaluate_dag(
        &mut self,
        client: ClientInfo,
        dag: ExecutionDAGData,
        callbacks: ExecutionDAGWatchSet,
    ) -> Result<(), Error> {
        // build the scheduler structures, insert the client in the list of working
        // clients and schedule all the already cached executions.
        let mut client_data = SchedulerClientData::new(client.name, dag, callbacks);
        for group in client_data.dag.execution_groups.values() {
            let missing_dep = client_data.missing_deps.entry(group.uuid).or_default();
            for exec in &group.executions {
                for input in exec.dependencies() {
                    let entry = client_data.input_of.entry(input).or_default();
                    entry.insert(group.uuid);
                    missing_dep.insert(input);
                }
            }
        }
        self.clients.insert(client.uuid, client_data);
        // the client may have sent and empty DAG
        self.check_completion(client.uuid)?;

        self.schedule_cached()?;
        self.assign_jobs()?;
        Ok(())
    }

    /// Handle the message of a file being ready.
    fn handle_file_ready(
        &mut self,
        client_uuid: ClientUuid,
        uuid: FileUuid,
        handle: FileStoreHandle,
    ) -> Result<(), Error> {
        if let Some(client) = self.clients.get_mut(&client_uuid) {
            client.file_handles.insert(uuid, handle);
            self.file_success(client_uuid, uuid)?;
            self.check_completion(client_uuid)?;
        } else {
            warn!("Client is gone");
        }
        Ok(())
    }

    /// Handle the completion of an execution on a worker.
    fn handle_worker_result(
        &mut self,
        worker: WorkerUuid,
        result: ExecutionResult,
        outputs: HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        let worker = match self.connected_workers.remove(&worker) {
            Some(worker) => worker,
            None => {
                warn!("Unknown worker {} completed a job", worker);
                return Ok(());
            }
        };
        let (client_uuid, group_uuid) = match worker.current_job {
            Some((client, exec, _)) => (client, exec),
            None => {
                warn!(
                    "Worker {} ({}) completed a job that wasn't doing",
                    worker.name, worker.uuid
                );
                return Ok(());
            }
        };
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            warn!("Worker completed execution but client is gone");
            self.assign_jobs()?;
            self.check_completion(client_uuid)?;
            return Ok(());
        };
        let group = client.dag.execution_groups[&group_uuid].clone();
        info!(
            "Worker {:?} completed execution group {}",
            worker, group.uuid
        );
        client.running_groups.remove(&group_uuid);
        self.exec_completed(client_uuid, &group, result, outputs)?;
        self.assign_jobs()?;
        self.check_completion(client_uuid)?;
        Ok(())
    }

    /// Handle the connection of a worker.
    fn handle_worker_connected(&mut self, uuid: WorkerUuid, name: String) -> Result<(), Error> {
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
        Ok(())
    }

    /// Handle the disconnection of a worker.
    fn handle_worker_disconnected(&mut self, uuid: WorkerUuid) -> Result<(), Error> {
        info!("Worker {} disconnected", uuid);
        if let Some(worker) = self.connected_workers.remove(&uuid) {
            // reschedule the job if the worker failed
            if let Some((client_uuid, job, _)) = worker.current_job {
                let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                    client
                } else {
                    warn!("Worker was doing something for a gone client");
                    return Ok(());
                };
                let priority = client.dag.execution_groups[&job].priority();
                self.ready_execs.push((priority, job, client_uuid));
                client.ready_groups.insert(job);
                client.running_groups.remove(&job);
            }
        }
        Ok(())
    }

    /// Handle the disconnection of a client.
    fn handle_client_disconnected(&mut self, client: ClientUuid) -> Result<(), Error> {
        info!("Client {} disconnected", client);
        if let Some(client) = self.clients.get(&client) {
            if !client.is_done() {
                warn!("The client's evaluation wasn't completed yet");
            }
        }
        self.clients.remove(&client);
        let mut remaining = BinaryHeap::new();
        while let Some((priority, exec, client)) = self.ready_execs.pop() {
            if self.clients.contains_key(&client) {
                remaining.push((priority, exec, client));
            }
        }
        self.ready_execs = remaining;
        // stop the jobs that are still running in the workers
        for (uuid, worker) in self.connected_workers.iter() {
            if let Some((owner, exec, _)) = worker.current_job {
                if owner == client {
                    warn!(
                        "Worker {} is doing {} owned by disconnected client, killing",
                        uuid, exec
                    );
                    self.worker_manager
                        .send(WorkerManagerInMessage::StopWorkerJob {
                            worker: *uuid,
                            job: exec,
                        })
                        .map_err(|e| format_err!("Failed to send job to worker: {:?}", e))?;
                }
            }
        }
        Ok(())
    }

    /// Handle the status request of a client.
    fn handle_status_request(&mut self, client_uuid: ClientUuid) -> Result<(), Error> {
        let mut ready_execs = 0;
        let mut waiting_execs = 0;
        for client in self.clients.values() {
            ready_execs += client.ready_groups.len();
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
                            let client = if let Some(client) = self.clients.get(&client_uuid) {
                                client
                            } else {
                                return None;
                            };
                            let exec = &client.dag.execution_groups[exec_uuid];
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

        if let Err(e) = self
            .executor
            .send((client_uuid, SchedulerExecutorMessageData::Status { status }))
        {
            warn!("Cannot send the status to the client: {:?}", e);
        }
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
        for group_uuid in client.input_of[&file].clone() {
            // do not skip the same execution twice
            if client.missing_deps.contains_key(&group_uuid) {
                client.missing_deps.remove(&group_uuid);
            } else {
                continue;
            }
            let group = &client.dag.execution_groups[&group_uuid];
            for exec in &group.executions {
                if client.callbacks.executions.contains(&exec.uuid) {
                    if let Err(e) = self.executor.send((
                        client_uuid,
                        SchedulerExecutorMessageData::ExecutionSkipped {
                            execution: exec.uuid,
                        },
                    )) {
                        warn!("Cannot tell the client the execution was skipped: {:?}", e);
                    }
                }
                for output in exec.outputs() {
                    failed_files.push((client_uuid, output));
                }
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
        for group_uuid in &client.input_of[&file] {
            let group = &client.dag.execution_groups[&group_uuid];
            if let Some(files) = client.missing_deps.get_mut(group_uuid) {
                files.remove(&file);
                if files.is_empty() {
                    client.missing_deps.remove(group_uuid);
                    self.ready_execs
                        .push((group.priority(), *group_uuid, client_uuid));
                    client.ready_groups.insert(*group_uuid);
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
        if let Err(e) = self.executor.send((
            client_uuid,
            SchedulerExecutorMessageData::FileReady {
                file,
                handle: client.file_handles[&file].clone(),
                successful: status,
                urgent: client.callbacks.urgent_files.contains(&file),
            },
        )) {
            warn!("Cannot send the file to the client: {:?}", e);
        }
        Ok(())
    }

    /// Mark an execution as completed, sending the notification to the client and marking all the
    /// produced files as done. Add the execution to the cache and schedule all the new executions
    /// that become ready.
    fn exec_completed(
        &mut self,
        client_uuid: ClientUuid,
        group: &ExecutionGroup,
        result: ExecutionResult,
        outputs: HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
            client
        } else {
            // client is gone, dont worry to much about it
            return Ok(());
        };
        for exec in &group.executions {
            if client.callbacks.executions.contains(&exec.uuid) {
                if let Err(e) = self.executor.send((
                    client_uuid,
                    SchedulerExecutorMessageData::ExecutionDone {
                        execution: exec.uuid,
                        result: result.clone(),
                    },
                )) {
                    warn!("Cannot tell the client the execution is done: {:?}", e);
                }
            }
        }
        for (uuid, handle) in outputs.iter() {
            client.file_handles.insert(*uuid, handle.clone());
        }

        let successful = ExecutionStatus::Success == result.status;
        // TODO: handle cache
        // match &result.status {
        //     ExecutionStatus::InternalError(_) => {} // do not cache internal errors
        //     _ => self.cache_execution(client_uuid, &group, outputs, result),
        // }
        if successful {
            for exec in &group.executions {
                for output in exec.outputs() {
                    self.file_success(client_uuid, output)?;
                }
            }
        } else {
            for exec in &group.executions {
                for output in exec.outputs() {
                    self.file_failed(client_uuid, output)?;
                }
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
        // let mut cached = Vec::new();

        for (priority, group_uuid, client_uuid) in self.ready_execs.iter() {
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
                not_cached.push((*priority, *group_uuid, *client_uuid));
                continue;
            }
            // TODO handle cache
            let group = dag.execution_groups[group_uuid].clone();
            not_cached.push((*priority, group.uuid, *client_uuid));
            // if !Scheduler::is_cacheable(&group, &cache_mode) {
            //     not_cached.push((*priority, group.uuid, *client_uuid));
            //     continue;
            // }
            // let result = self
            //     .cache
            //     .get(&group, &client.file_handles, self.file_store.as_ref());
            // match result {
            //     CacheResult::Hit { result, outputs } => {
            //         info!("Execution {} is a cache hit!", group.uuid);
            //         client.ready_groups.remove(&group.uuid);
            //         cached.push((*client_uuid, group, result, outputs));
            //     }
            //     CacheResult::Miss => {
            //         not_cached.push((*priority, group.uuid, *client_uuid));
            //     }
            // }
        }

        self.ready_execs = not_cached;
        // for (client, exec, result, outputs) in cached.into_iter() {
        //     self.exec_completed(client, &exec, result, outputs)?;
        // }

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
            let (_, group_uuid, client_uuid) = match self.ready_execs.pop() {
                Some(exec) => exec,
                None => break,
            };
            trace!("Assigning {} to worker {}", group_uuid, worker_uuid);
            worker.current_job = Some((client_uuid, group_uuid, Instant::now()));
            let client = if let Some(client) = self.clients.get_mut(&client_uuid) {
                client
            } else {
                // client is gone, dont worry to much about it
                continue;
            };
            client.ready_groups.remove(&group_uuid);
            client.running_groups.insert(group_uuid);
            let group = &client.dag.execution_groups[&group_uuid];
            let mut dep_keys: HashMap<FileUuid, FileStoreKey> = HashMap::new();
            for exec in &group.executions {
                for file in exec.dependencies() {
                    let handle = client
                        .file_handles
                        .get(&file)
                        .unwrap_or_else(|| panic!("Unknown file key of {}", file))
                        .key()
                        .clone();
                    dep_keys.insert(file, handle);
                }
            }
            let job = WorkerJob {
                group: group.clone(),
                dep_keys,
            };
            self.worker_manager
                .send(WorkerManagerInMessage::WorkerJob {
                    worker: *worker_uuid,
                    job,
                })
                .map_err(|e| format_err!("Failed to send job to worker: {:?}", e))?;
            for exec in &group.executions {
                if client.callbacks.executions.contains(&exec.uuid) {
                    if let Err(e) = self.executor.send((
                        client_uuid,
                        SchedulerExecutorMessageData::ExecutionStarted {
                            execution: exec.uuid,
                            worker: *worker_uuid,
                        },
                    )) {
                        warn!("Cannot tell the client the execution started: {:?}", e);
                    }
                }
            }
        }
        Ok(())
    }
}
