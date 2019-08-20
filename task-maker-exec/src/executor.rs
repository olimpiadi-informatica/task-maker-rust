use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use failure::{Error, Fail};
use serde::{Deserialize, Serialize};
use task_maker_dag::*;
use task_maker_store::*;

use crate::proto::*;
use crate::*;
use std::ops::DerefMut;
use std::thread::JoinHandle;

/// An error in the DAG structure.
#[derive(Debug, Fail)]
pub enum DAGError {
    /// A file is used as input in an execution but it's missing, or a callback is registered on a
    /// file but it's missing.
    #[fail(display = "missing file {} ({})", description, uuid)]
    MissingFile {
        /// The UUID of the missing file.
        uuid: FileUuid,
        /// The description of the missing file.
        description: String,
    },
    /// A callback is registered on an execution but it's missing.
    #[fail(display = "missing execution {}", uuid)]
    MissingExecution {
        /// The UUID of the missing execution.
        uuid: ExecutionUuid,
    },
    /// There is a dependency cycle in the DAG.
    #[fail(
        display = "detected dependency cycle, '{}' is in the cycle",
        description
    )]
    CycleDetected {
        /// The description of an execution inside the cycle.
        description: String,
    },
    /// There is a duplicate execution UUID.
    #[fail(display = "duplicate execution UUID {}", uuid)]
    DuplicateExecutionUUID {
        /// The duplicated UUID.
        uuid: ExecutionUuid,
    },
    /// There is a duplicate file UUID.
    #[fail(display = "duplicate file UUID {}", uuid)]
    DuplicateFileUUID {
        /// The duplicated UUID.
        uuid: FileUuid,
    },
}

/// List of the _interesting_ files and executions, only the callbacks listed here will be called by
/// the server. Every other callback is not sent to the client for performance reasons.
#[derive(Debug, Serialize, Deserialize)]
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
    /// The FileStoreKeys the worker has to know to start the evaluation.
    pub dep_keys: HashMap<FileUuid, FileStoreKey>,
}

/// The state of a waiting worker.
#[derive(Debug, Clone)]
pub(crate) enum WorkerWaitingState {
    /// The worker is waiting for some job which is not ready yet.
    Waiting,
    /// The worker got some job.
    GotJob(WorkerJob),
    /// The computation ended, the worker should exit now.
    Exit
}

/// State of a worker as seen by the server.
#[derive(Debug)]
pub(crate) struct WorkerState {
    /// Name of the worker.
    pub name: String,
    /// Current job of the worker.
    pub job: Mutex<WorkerWaitingState>,
    /// Conditional variable the worker thread on the server is waiting for some work to be ready.
    /// Will be waked up when a job is present.
    pub cv: Condvar,
}

/// Internal state of the Executor.
#[derive(Debug)]
pub(crate) struct ExecutorData {
    /// The DAG which is currently being evaluated.
    pub dag: Option<ExecutionDAGData>,
    /// The sets of the callbacks the client is interested in.
    pub callbacks: Option<ExecutionDAGWatchSet>,
    /// A channel for sending messages to the client.
    pub client_sender: Option<ChannelSender>,
    /// The state of the connected workers.
    pub workers: HashMap<WorkerUuid, Arc<WorkerState>>,
    /// The priority queue of the ready tasks, waiting for the workers.
    pub ready_execs: BinaryHeap<ExecutionUuid>,
    /// The list of tasks waiting for some dependencies, each value here is positive, when a task is
    /// ready it's removed from the map.
    pub missing_deps: HashMap<ExecutionUuid, usize>,
    /// The list of tasks that depends on a file, this is a lookup table for when the files become
    /// ready.
    pub dependents: HashMap<FileUuid, Vec<ExecutionUuid>>,
    /// A reference to the server's [`FileStore`](../task_maker_store/struct.FileStore.html).
    pub file_store: Arc<Mutex<FileStore>>,
    /// The list of known [`FileStoreKey`](../task_maker_store/struct.FileStoreKey.html)s for the
    /// current DAG.
    pub file_keys: HashMap<FileUuid, FileStoreKey>,
    /// True if the executor is shutting down and no more worker should be accepted.
    pub shutting_down: bool,
}

/// The current status of the `Executor`, this is sent to the user when the server status is asked.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutorStatus {
    /// List of the connected workers with their uuid, name and if they have some work.
    pub connected_workers: Vec<(WorkerUuid, String, bool)>,
    /// Number of running DAGs by the server.
    pub running_dags: usize,
    /// Number of executions waiting for workers.
    pub ready_execs: usize,
    /// Number of executions waiting for dependencies.
    pub waiting_execs: usize,
}

/// The `Executor` is the main component of the server, this will receive the DAG to evaluate and
/// will schedule the tasks to the workers, sending to the client the responses.
pub(crate) struct Executor {
    /// The internals of the executor.
    pub data: Arc<Mutex<ExecutorData>>,
}

impl Executor {
    /// Make a new `Executor` based on the specified
    /// [`FileStore`](../task_maker_store/struct.FileStore.html).
    pub fn new(file_store: Arc<Mutex<FileStore>>) -> Executor {
        Executor {
            data: Arc::new(Mutex::new(ExecutorData::new(file_store))),
        }
    }

    /// Connect a new worker to the server, this will spawn a new thread that will manage the
    /// connection with the worker.
    pub fn add_worker(&mut self, worker: WorkerConn) -> JoinHandle<()> {
        let data = self.data.clone();
        thread::Builder::new()
            .name(format!("Executor worker thread for {}", worker))
            .spawn(move || {
                worker_thread(data, worker).expect("Worker failed");
            })
            .expect("Failed to spawn executor worker thread")
    }

    /// Starts the `Executor` for a client, this will block and will manage the communication with
    /// the client.
    ///
    /// * `sender` - A channel that sends messages to the client.
    /// * `receiver` - A channel that receives messages from the client.
    pub fn evaluate(
        &mut self,
        sender: ChannelSender,
        receiver: ChannelReceiver,
    ) -> Result<(), Error> {
        loop {
            let message = deserialize_from::<ExecutorClientMessage>(&receiver);
            match message {
                Ok(ExecutorClientMessage::Evaluate { dag, callbacks }) => {
                    info!("Want to evaluate a DAG!");
                    if let Err(e) = check_dag(&dag, &callbacks) {
                        warn!("Invalid DAG: {:?}", e);
                        serialize_into(&ExecutorServerMessage::Error(e.to_string()), &sender)?;
                        break;
                    } else {
                        trace!("DAG looks valid!");
                    }
                    {
                        let mut data = self.data.lock().unwrap();
                        data.dag = Some(dag);
                        data.callbacks = Some(callbacks);
                        data.client_sender = Some(sender.clone());
                        Scheduler::setup(data.deref_mut());
                        Scheduler::schedule(data.deref_mut());
                    }
                    let mut data = self.data.lock().unwrap();
                    let mut ready_files = vec![];
                    let provided_files = data.dag.as_ref().unwrap().provided_files.clone();
                    let file_store = data.file_store.clone();
                    let mut file_store = file_store.lock().unwrap();
                    for (uuid, file) in provided_files.into_iter() {
                        if !file_store.has_key(&file.key) {
                            serialize_into(&ExecutorServerMessage::AskFile(uuid), &sender)?;
                        } else {
                            file_store.persist(&file.key)?;
                            data.file_keys.insert(uuid, file.key.clone());
                            ready_files.push(uuid);
                            trace!("File {} already in store!", uuid);
                        }
                    }
                    for file in ready_files.into_iter() {
                        Scheduler::file_ready(data.deref_mut(), file);
                    }
                }
                Ok(ExecutorClientMessage::ProvideFile(uuid, key)) => {
                    info!("Client sent: {} {:?}", uuid, key);
                    let mut data = self.data.lock().unwrap();
                    if data.dag.is_none() {
                        warn!("Provided file before the DAG!");
                        serialize_into(
                            &ExecutorServerMessage::Error("Provided file before the DAG!".into()),
                            &sender,
                        )?;
                        break;
                    }
                    data.file_store
                        .lock()
                        .unwrap()
                        .store(&key, ChannelFileIterator::new(&receiver))?;
                    data.file_keys.insert(uuid, key.clone());
                    Scheduler::file_ready(data.deref_mut(), uuid);
                }
                Ok(ExecutorClientMessage::Status) => {
                    info!("Client asking for the status");
                    let data = self.data.lock().unwrap();
                    serialize_into(
                        &ExecutorServerMessage::Status(ExecutorStatus {
                            connected_workers: data
                                .workers
                                .iter()
                                .map(|(uuid, worker)| {
                                    (
                                        *uuid,
                                        worker.name.clone(),
                                        match *worker.job.lock().unwrap() {
                                            WorkerWaitingState::GotJob(_) => true,
                                            _ => false
                                        }
                                    )
                                })
                                .collect(),
                            running_dags: data.dag.is_some() as usize,
                            ready_execs: data.ready_execs.len(),
                            waiting_execs: data.missing_deps.len(),
                        }),
                        &sender,
                    )?;
                }
                Ok(ExecutorClientMessage::Stop) => {
                    // TODO stop and kill all the workers
                    break;
                }
                Err(e) => {
                    // TODO stop and kill all the workers
                    let cause = e.find_root_cause().to_string();
                    if cause == "receiving on a closed channel" {
                        trace!("Connection closed: {}", cause);
                        break;
                    } else {
                        error!("Connection error: {}", cause);
                    }
                }
            }
        }
        stop_all_workers(self.data.lock().unwrap().deref_mut());
        Ok(())
    }
}

impl ExecutorData {
    /// Make a new ExecutorData based on the specified
    /// [`FileStore`](../task_maker_store/struct.FileStore.html).
    fn new(file_store: Arc<Mutex<FileStore>>) -> ExecutorData {
        ExecutorData {
            dag: None,
            callbacks: None,
            client_sender: None,
            workers: HashMap::new(),
            ready_execs: BinaryHeap::new(),
            missing_deps: HashMap::new(),
            dependents: HashMap::new(),
            file_store,
            file_keys: HashMap::new(),
            shutting_down: false,
        }
    }
}

/// Thread function that manages the connection with a worker. This function is intended to be
/// called as a thread body, this will block the thread until the worker disconnects.
fn worker_thread(executor: Arc<Mutex<ExecutorData>>, conn: WorkerConn) -> Result<(), Error> {
    trace!("Server connected to worker {}", conn);

    loop {
        let message = deserialize_from::<WorkerClientMessage>(&conn.receiver);
        match message {
            Ok(WorkerClientMessage::GetWork) => {
                trace!("Worker {} ready for work", conn);
                assert!(!executor.lock().unwrap().workers.contains_key(&conn.uuid));
                {
                    let mut executor = executor.lock().unwrap();
                    // disallow new workers if the executor is shutting down
                    if executor.shutting_down {
                        break;
                    }
                    executor.workers.insert(
                        conn.uuid,
                        Arc::new(WorkerState {
                            name: conn.name.clone(),
                            job: Mutex::new(WorkerWaitingState::Waiting),
                            cv: Condvar::new(),
                        }),
                    );

                    Scheduler::schedule(executor.deref_mut());
                }
                match wait_for_work(executor.clone(), &conn.uuid) {
                    WorkerWaitingState::GotJob(job) => {
                        serialize_into(&WorkerServerMessage::Work(Box::new(job)), &conn.sender).unwrap();
                    }
                    WorkerWaitingState::Exit => {
                        info!("Worker {} asked to exit", conn);
                        break;
                    }
                    _ => unreachable!("wait_for_work returned without reason")
                }
            }
            Ok(WorkerClientMessage::WorkerDone(result)) => {
                info!("Worker {} completed with: {:?}", conn, result);
                let mut data = executor.lock().unwrap();
                let exec = if let WorkerWaitingState::GotJob(job) = data
                    .workers
                    .get(&conn.uuid)
                    .expect("Worker disappeared")
                    .job
                    .lock()
                    .unwrap().clone() {
                    job
                } else {
                    panic!("Worker job disappeared");
                };
                let exec_uuid = exec.clone().execution.uuid;
                data.workers.remove(&conn.uuid);
                if data
                    .callbacks
                    .as_ref()
                    .unwrap()
                    .executions
                    .contains(&exec_uuid)
                {
                    serialize_into(
                        &ExecutorServerMessage::NotifyDone(exec_uuid, result.clone()),
                        data.client_sender.as_ref().unwrap(),
                    )?;
                }
                match result.status {
                    ExecutionStatus::Success => {}
                    _ => Scheduler::exec_failed(data.deref_mut(), exec_uuid),
                }
            }
            Ok(WorkerClientMessage::ProvideFile(uuid, key)) => {
                info!("Worker provided file {} {:?}", uuid, key);
                let mut data = executor.lock().unwrap();
                data.file_store
                    .lock()
                    .unwrap()
                    .store(&key, ChannelFileIterator::new(&conn.receiver))?;
                data.file_keys.insert(uuid, key.clone());
                Scheduler::file_ready(data.deref_mut(), uuid);
                if data.callbacks.as_ref().unwrap().files.contains(&uuid) {
                    serialize_into(
                        &ExecutorServerMessage::ProvideFile(uuid),
                        &data.client_sender.as_ref().unwrap(),
                    )?;
                    let path = data.file_store.lock().unwrap().get(&key)?.unwrap();
                    ChannelFileSender::send(&path, &data.client_sender.as_ref().unwrap())?;
                }
            }
            Ok(WorkerClientMessage::AskFile(uuid)) => {
                info!("Worker asked for {}", uuid);
                let data = executor.lock().unwrap();
                let key = data
                    .file_keys
                    .get(&uuid)
                    .expect("Worker is asking unknown file")
                    .clone();
                let path = data
                    .file_store
                    .lock()
                    .unwrap()
                    .get(&key)?
                    .expect("File not present in store");
                serialize_into(&WorkerServerMessage::ProvideFile(uuid, key), &conn.sender)?;
                ChannelFileSender::send(&path, &conn.sender)?;
            }
            Err(e) => {
                let cause = e.find_root_cause().to_string();
                if cause == "receiving on a closed channel" {
                    break;
                } else {
                    error!("Connection error: {}", cause);
                }
            }
        }
    }

    let mut data = executor.lock().unwrap();
    data.workers.remove(&conn.uuid);
    info!("Removed worker {} from pool", conn);
    Scheduler::schedule(data.deref_mut());

    Ok(())
}

/// Block the thread until there is something to do for this worker.
fn wait_for_work(executor: Arc<Mutex<ExecutorData>>, uuid: &WorkerUuid) -> WorkerWaitingState {
    let worker = &*executor.lock().unwrap().workers[&uuid].clone();
    let mut job = worker.job.lock().unwrap();
    while let WorkerWaitingState::Waiting = *job {
        job = worker.cv.wait(job).unwrap();
    }
    job.clone()
}

/// Stop all the workers by removing their job and notifying them of the change. This wont stop the
/// worker in the middle of a job. No more workers will be accepted.
pub(crate) fn stop_all_workers(executor_data: &mut ExecutorData) {
    executor_data.shutting_down = true;
    for worker in executor_data.workers.values() {
        *worker.job.lock().unwrap() = WorkerWaitingState::Exit;
        worker.cv.notify_one();
    }
}

/// Validate the DAG checking if all the required pieces are present and they actually make a DAG.
/// It's checked that no duplicated UUID are present, no files are missing, all the executions are
/// reachable and no cycles are present.
fn check_dag(dag: &ExecutionDAGData, callbacks: &ExecutionDAGWatchSet) -> Result<(), DAGError> {
    let mut dependencies: HashMap<FileUuid, Vec<ExecutionUuid>> = HashMap::new();
    let mut num_dependencies: HashMap<ExecutionUuid, usize> = HashMap::new();
    let mut known_files: HashSet<FileUuid> = HashSet::new();
    let mut ready_execs: VecDeque<ExecutionUuid> = VecDeque::new();
    let mut ready_files: VecDeque<FileUuid> = VecDeque::new();

    let mut add_dependency = |file: FileUuid, exec: ExecutionUuid| {
        dependencies
            .entry(file)
            .or_insert_with(|| vec![])
            .push(exec);
    };

    // add the executions and check for duplicated UUIDs
    for exec_uuid in dag.executions.keys() {
        let exec = dag.executions.get(exec_uuid).expect("No such exec");
        let deps = exec.dependencies();
        let count = deps.len();
        for dep in deps.into_iter() {
            add_dependency(dep, *exec_uuid);
        }
        for out in exec.outputs().into_iter() {
            if !known_files.insert(out) {
                return Err(DAGError::DuplicateFileUUID { uuid: out });
            }
        }
        if num_dependencies.insert(*exec_uuid, count).is_some() {
            return Err(DAGError::DuplicateExecutionUUID { uuid: *exec_uuid });
        }
        if count == 0 {
            ready_execs.push_back(exec_uuid.clone());
        }
    }
    // add the provided files
    for uuid in dag.provided_files.keys() {
        ready_files.push_back(uuid.clone());
        if !known_files.insert(uuid.clone()) {
            return Err(DAGError::DuplicateFileUUID { uuid: *uuid });
        }
    }
    // visit the DAG for finding the unreachable executions / cycles
    while !ready_execs.is_empty() || !ready_files.is_empty() {
        for file in ready_files.drain(..) {
            if !dependencies.contains_key(&file) {
                continue;
            }
            for exec in dependencies[&file].iter() {
                let num_deps = num_dependencies
                    .get_mut(&exec)
                    .expect("num_dependencies of an unknown execution");
                assert_ne!(
                    *num_deps, 0,
                    "num_dependencis is going to be negative for {}",
                    exec
                );
                *num_deps -= 1;
                if *num_deps == 0 {
                    ready_execs.push_back(exec.clone());
                }
            }
        }
        for exec_uuid in ready_execs.drain(..) {
            let exec = dag.executions.get(&exec_uuid).expect("No such exec");
            for file in exec.outputs().into_iter() {
                ready_files.push_back(file);
            }
        }
    }
    // search for unreachable execution / cycles
    for (exec_uuid, count) in num_dependencies.iter() {
        if *count == 0 {
            continue;
        }
        let exec = &dag.executions[&exec_uuid];
        for dep in exec.dependencies().iter() {
            if !known_files.contains(dep) {
                return Err(DAGError::MissingFile {
                    uuid: *dep,
                    description: format!("Dependency of '{}'", exec.description),
                });
            }
        }
        return Err(DAGError::CycleDetected {
            description: exec.description.clone(),
        });
    }
    // check the file callbacks
    for file in callbacks.files.iter() {
        if !known_files.contains(&file) {
            return Err(DAGError::MissingFile {
                uuid: *file,
                description: "File required by a callback".to_owned(),
            });
        }
    }
    // check the execution callbacks
    for exec in callbacks.executions.iter() {
        if !num_dependencies.contains_key(&exec) {
            return Err(DAGError::MissingExecution { uuid: *exec });
        }
    }
    Ok(())
}
