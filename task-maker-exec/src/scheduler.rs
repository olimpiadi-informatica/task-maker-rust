use crate::proto::WorkerServerMessage;
use crate::{
    serialize_into, ChannelSender, ExecutionDAGWatchSet, ExecutorStatus, ExecutorWorkerStatus,
    WorkerJob,
};
use failure::{format_err, Error};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use task_maker_cache::{Cache, CacheResult};
use task_maker_dag::{
    CacheMode, Execution, ExecutionDAGData, ExecutionResult, ExecutionStatus, ExecutionUuid,
    FileUuid, WorkerUuid,
};
use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey};

/// A `Scheduler` is a service that is able to orchestrate the execution of a DAG, sending the jobs
/// to the workers, listening for events and managing the cache of the executions.
pub struct Scheduler {
    /// The DAG the `Scheduler` is currently working on.
    dag: Option<ExecutionDAGData>,
    /// The set of callbacks the client is interested in.
    callbacks: Option<ExecutionDAGWatchSet>,

    /// The priority queue of the ready tasks, waiting for the workers.
    ready_execs: BinaryHeap<ExecutionUuid>,
    /// The list of tasks waiting for some dependencies, each with the list of missing files, when a
    /// task is ready it's removed from the map.
    missing_deps: HashMap<ExecutionUuid, HashSet<FileUuid>>,
    /// The set of executions that depends on a file, this is a lookup table for when the files
    /// become ready.
    input_of: HashMap<FileUuid, HashSet<ExecutionUuid>>,

    /// The list of known [`FileStoreHandle`](../task_maker_store/struct.FileStoreHandle.html)s
    /// for the current DAG. Storing them here prevents the `FileStore` from flushing them away.
    file_handles: HashMap<FileUuid, FileStoreHandle>,

    /// The cache of the executions.
    cache: Cache,
    /// A reference to the server's [`FileStore`](../task_maker_store/struct.FileStore.html).
    file_store: Arc<FileStore>,
    /// The list of the workers that are either ready for some work or already working on a job.
    connected_workers: HashMap<WorkerUuid, ConnectedWorker>,
    /// The channel to use to send messages to the executor.
    executor: Sender<SchedulerOutMessage>,
}

/// The state of a connected worker.
#[derive(Debug)]
struct ConnectedWorker {
    /// The uuid of the worker.
    uuid: WorkerUuid,
    /// The name of the worker.
    name: String,
    /// The channel to use to send messages to the worker.
    sender: ChannelSender,
    /// The job the worker is currently working on, with the instant of the start.
    current_job: Option<(ExecutionUuid, Instant)>,
}

/// Messages that the scheduler can receive.
#[derive(Debug)]
pub enum SchedulerInMessage {
    /// The executor is asking to evaluate a DAG.
    DAG {
        /// The DAG to evaluate.
        dag: ExecutionDAGData,
        /// The set of callbacks the client is interested in.
        callbacks: ExecutionDAGWatchSet,
    },
    /// A new file is ready in the store.
    FileReady {
        /// The uuid of the file inside the DAG.
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
        /// The channel to use to send messages to the worker.
        sender: ChannelSender,
    },
    /// A previously ready worker is not ready anymore.
    WorkerDisconnected {
        /// The uuid of the worker that has disconnected.
        uuid: WorkerUuid,
    },
    /// The executor is asking for the status of the scheduler.
    Status,
    /// The executor is asking to exit.
    Exit,
}

/// Messages that the `Scheduler` sends to the `Executor`.
#[derive(Debug)]
pub enum SchedulerOutMessage {
    /// An execution has started on a specific worker.
    ExecutionStarted(ExecutionUuid, WorkerUuid),
    /// An execution has been completed.
    ExecutionDone(ExecutionUuid, ExecutionResult),
    /// An execution has been skipped.
    ExecutionSkipped(ExecutionUuid),
    /// A file is ready in the store. The `bool` is `true` if it comes from a successful execution.
    FileReady(FileUuid, FileStoreHandle, bool),
    /// The status of the scheduler.
    Status(ExecutorStatus<Duration>),
}

impl Scheduler {
    /// Make a new scheduler bound to the specified executor.
    pub fn new(
        cache: Cache,
        file_store: Arc<FileStore>,
        executor: Sender<SchedulerOutMessage>,
    ) -> Scheduler {
        Scheduler {
            dag: None,
            callbacks: None,
            ready_execs: BinaryHeap::new(),
            missing_deps: HashMap::new(),
            file_handles: HashMap::new(),
            cache,
            file_store,
            connected_workers: HashMap::new(),
            executor,
            input_of: HashMap::new(),
        }
    }

    /// Consume the `Scheduler` starting the scheduling process and returning after the evaluation
    /// has been completed.
    pub fn work(mut self, recv: Receiver<SchedulerInMessage>) -> Result<(), Error> {
        while self.dag.is_none() || !self.is_done() {
            let message = recv.recv();
            match message {
                Ok(SchedulerInMessage::DAG { dag, callbacks }) => {
                    info!("Scheduler received a new DAG");
                    let mut input_of: HashMap<FileUuid, HashSet<ExecutionUuid>> = HashMap::new();
                    for exec in dag.executions.values() {
                        let missing_dep = self.missing_deps.entry(exec.uuid).or_default();
                        for input in exec.dependencies().iter() {
                            let entry = input_of.entry(*input).or_default();
                            entry.insert(exec.uuid);
                            missing_dep.insert(*input);
                        }
                    }
                    self.dag = Some(dag);
                    self.callbacks = Some(callbacks);
                    self.input_of = input_of;
                }
                Ok(SchedulerInMessage::FileReady { uuid, handle }) => {
                    info!("Client sent a file {:?}", uuid);
                    self.file_handles.insert(uuid, handle);
                    self.file_success(uuid)?;
                }
                Ok(SchedulerInMessage::WorkerResult {
                    worker,
                    result,
                    outputs,
                }) => {
                    let worker = match self.connected_workers.remove(&worker) {
                        Some(worker) => worker,
                        None => {
                            warn!("Unknown worker {} completed a job", worker);
                            continue;
                        }
                    };
                    let execution_uuid = match worker.current_job {
                        Some((uuid, _)) => uuid,
                        None => {
                            warn!(
                                "Worker {} ({}) completed a job that wasn't doing",
                                worker.name, worker.uuid
                            );
                            continue;
                        }
                    };
                    let execution = self
                        .dag
                        .as_ref()
                        .ok_or_else(|| format_err!("DAG is gone"))?
                        .executions[&execution_uuid]
                        .clone();
                    info!("Worker {:?} completed execution {}", worker, execution.uuid);
                    self.exec_completed(&execution, result, outputs)?;
                    self.assign_jobs()?;
                }
                Ok(SchedulerInMessage::WorkerConnected { uuid, name, sender }) => {
                    info!("Worker {} ({}) connected", name, uuid);
                    self.connected_workers.insert(
                        uuid,
                        ConnectedWorker {
                            uuid,
                            name,
                            sender,
                            current_job: None,
                        },
                    );
                    self.assign_jobs()?;
                }
                Ok(SchedulerInMessage::WorkerDisconnected { uuid }) => {
                    info!("Worker {} disconnected", uuid);
                    if let Some(worker) = self.connected_workers.remove(&uuid) {
                        if let Some((job, _)) = worker.current_job {
                            self.ready_execs.push(job);
                        }
                    }
                }
                Ok(SchedulerInMessage::Status) => {
                    let dag = self
                        .dag
                        .as_ref()
                        .ok_or_else(|| format_err!("DAG is gone"))?;
                    let status = ExecutorStatus {
                        connected_workers: self
                            .connected_workers
                            .values()
                            .map(|worker| ExecutorWorkerStatus {
                                uuid: worker.uuid,
                                name: worker.name.clone(),
                                current_job: worker.current_job.as_ref().map(|(exec, start)| {
                                    (dag.executions[&exec].description.clone(), start.elapsed())
                                }),
                            })
                            .collect(),
                        ready_execs: self.ready_execs.len(),
                        waiting_execs: self.missing_deps.len(),
                    };
                    self.executor.send(SchedulerOutMessage::Status(status))?;
                }
                Ok(SchedulerInMessage::Exit) => {
                    break;
                }
                Err(_) => {
                    // the executor has dropped the sender
                    break;
                }
            }
        }
        debug!("Scheduler exited");
        Ok(())
    }

    /// Whether the evaluation of the DAG has been completed.
    fn is_done(&self) -> bool {
        if !self.ready_execs.is_empty() {
            return false;
        }
        if !self.missing_deps.is_empty() {
            return false;
        }
        for worker in self.connected_workers.values() {
            if worker.current_job.is_some() {
                return false;
            }
        }
        true
    }

    /// Mark a file as failed, skipping all the executions that depends on it (even transitively).
    /// This will also send the file to the client, if needed.
    fn file_failed(&mut self, file: FileUuid) -> Result<(), Error> {
        self.send_file(file, false)?;
        if !self.input_of.contains_key(&file) {
            return Ok(());
        }
        for exec in self.input_of[&file].clone() {
            // do not skip the same execution twice
            if self.missing_deps.contains_key(&exec) {
                self.missing_deps.remove(&exec);
            } else {
                continue;
            }
            if self
                .callbacks
                .as_ref()
                .ok_or_else(|| format_err!("Callbacks are gone"))?
                .executions
                .contains(&exec)
            {
                self.executor
                    .send(SchedulerOutMessage::ExecutionSkipped(exec))?;
            }
            let exec = &self
                .dag
                .as_ref()
                .ok_or_else(|| format_err!("DAG is gone"))?
                .executions[&exec];
            for output in exec.outputs() {
                self.file_failed(output)?;
            }
        }
        Ok(())
    }

    /// Mark a file as successful and schedule all the executions that become ready.
    /// This will also send the file to the client, if needed.
    fn file_success(&mut self, file: FileUuid) -> Result<(), Error> {
        self.send_file(file, true)?;
        if !self.input_of.contains_key(&file) {
            return Ok(());
        }
        for exec in &self.input_of[&file] {
            if self.missing_deps.contains_key(exec) {
                self.missing_deps.get_mut(exec).unwrap().remove(&file);
                if self.missing_deps[exec].is_empty() {
                    self.missing_deps.remove(exec);
                    self.ready_execs.push(*exec);
                }
            }
        }
        self.schedule_cached()?;
        self.assign_jobs()?;
        Ok(())
    }

    /// Send a file to the client if its uuid is included in the callbacks.
    fn send_file(&self, file: FileUuid, status: bool) -> Result<(), Error> {
        if !self
            .callbacks
            .as_ref()
            .ok_or_else(|| format_err!("Callbacks are gone"))?
            .files
            .contains(&file)
        {
            return Ok(());
        }
        if !self.file_handles.contains_key(&file) {
            return Ok(());
        }
        self.executor.send(SchedulerOutMessage::FileReady(
            file,
            self.file_handles[&file].clone(),
            status,
        ))?;
        Ok(())
    }

    /// Mark an execution as completed, sending the notification to the client and marking all the
    /// produced files as done. Add the execution to the cache and schedule all the new executions
    /// that become ready.
    fn exec_completed(
        &mut self,
        execution: &Execution,
        result: ExecutionResult,
        outputs: HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        if self
            .callbacks
            .as_ref()
            .ok_or_else(|| format_err!("Callbacks are gone"))?
            .executions
            .contains(&execution.uuid)
        {
            self.executor.send(SchedulerOutMessage::ExecutionDone(
                execution.uuid,
                result.clone(),
            ))?;
        }
        for (uuid, handle) in outputs.iter() {
            self.file_handles.insert(*uuid, handle.clone());
        }
        let successful = ExecutionStatus::Success == result.status;
        self.cache_execution(&execution, outputs, result);
        if successful {
            for output in execution.outputs() {
                self.file_success(output)?;
            }
        } else {
            for output in execution.outputs() {
                self.file_failed(output)?;
            }
        }
        self.schedule_cached()?;
        Ok(())
    }

    /// Store an execution in the cache.
    fn cache_execution(
        &mut self,
        execution: &Execution,
        outputs: HashMap<FileUuid, FileStoreHandle>,
        result: ExecutionResult,
    ) {
        let mut file_keys: HashMap<FileUuid, FileStoreKey> = execution
            .dependencies()
            .iter()
            .map(|f| (*f, self.file_handles[f].key().clone()))
            .collect();
        for output in execution.outputs() {
            file_keys.insert(output, outputs[&output].key().clone());
        }
        self.cache.insert(execution, &self.file_handles, result);
    }

    /// Look at all the ready executions and mark as completed all the ones that are inside the
    /// cache.
    fn schedule_cached(&mut self) -> Result<(), Error> {
        let cache_mode = &self
            .dag
            .as_ref()
            .ok_or_else(|| format_err!("DAG is gone"))?
            .config
            .cache_mode;
        // disable the cache
        if let CacheMode::Nothing = cache_mode {
            return Ok(());
        }

        let mut not_cached = BinaryHeap::new();
        let mut cached = Vec::new();

        for exec in self.ready_execs.iter() {
            let exec = self
                .dag
                .as_ref()
                .ok_or_else(|| format_err!("DAG is gone"))?
                .executions[exec]
                .clone();
            if !self.is_cacheable(&exec, &cache_mode) {
                not_cached.push(exec.uuid);
                continue;
            }
            let result = self
                .cache
                .get(&exec, &self.file_handles, self.file_store.as_ref());
            match result {
                CacheResult::Hit { result, outputs } => {
                    info!("Execution {} is a cache hit!", exec.uuid);
                    cached.push((exec, result, outputs));
                }
                CacheResult::Miss => {
                    not_cached.push(exec.uuid);
                }
            }
        }

        self.ready_execs = not_cached;
        for (exec, result, outputs) in cached.into_iter() {
            self.exec_completed(&exec, result, outputs)?;
        }

        Ok(())
    }

    /// Whether an execution is eligible to be fetch from the cache.
    fn is_cacheable(&self, execution: &Execution, cache_mode: &CacheMode) -> bool {
        if let (CacheMode::Except(set), Some(tag)) = (cache_mode, execution.tag.as_ref()) {
            if set.contains(tag) {
                return false;
            }
        }
        true
    }

    /// Give to each free worker a job from the ready executions.
    fn assign_jobs(&mut self) -> Result<(), Error> {
        // borrow connected_workers as mut, file_handles as not mut
        let file_handles = &self.file_handles;
        for (worker_uuid, worker) in self.connected_workers.iter_mut() {
            if worker.current_job.is_some() {
                continue;
            }
            let exec = match self.ready_execs.pop() {
                Some(exec) => exec,
                None => break,
            };
            worker.current_job = Some((exec, Instant::now()));
            let execution = self
                .dag
                .as_ref()
                .ok_or_else(|| format_err!("DAG is gone"))?
                .executions[&exec]
                .clone();
            let dep_keys = execution
                .dependencies()
                .iter()
                .map(|k| {
                    (
                        *k,
                        file_handles
                            .get(&k)
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
            serialize_into(&WorkerServerMessage::Work(Box::new(job)), &worker.sender)?;
            if self
                .callbacks
                .as_ref()
                .ok_or_else(|| format_err!("Callbacks are gone"))?
                .executions
                .contains(&exec)
            {
                self.executor
                    .send(SchedulerOutMessage::ExecutionStarted(exec, *worker_uuid))?;
            }
        }
        Ok(())
    }
}
