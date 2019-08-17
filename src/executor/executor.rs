use crate::execution::*;
use crate::executor::scheduler::Scheduler;
use crate::executor::*;
use task_maker_store::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// A job that is sent to a worker, this should include all the information
/// the worker needs to start the evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerJob {
    /// What the worker should do
    pub execution: Execution,
    /// The FileStoreKeys the worker has to know to start the evaluation
    pub dep_keys: HashMap<FileUuid, FileStoreKey>,
}

/// The result of a worker job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    /// The result of the evaluation
    pub result: ExecutionResult,
}

/// Messages that the client sends to the server
#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorClientMessage {
    /// The client is asking to evaluate a DAG
    Evaluate {
        dag: ExecutionDAGData,
        callbacks: ExecutionDAGCallbacks,
    },
    /// The client is providing a file. After this message there is a protocol
    /// switch for the file transmission
    ProvideFile(FileUuid, FileStoreKey),
    /// The client is asking to stop the evaluation
    Stop,
    /// The client is asking for the server status
    Status,
}

/// Messages that the server sends to the client
#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorServerMessage {
    /// The server needs the file with that Uuid
    AskFile(FileUuid),
    /// The server is sending a file. After this message there is a protocol
    /// switch for the file transmission
    ProvideFile(FileUuid),
    /// The execution has started on a worker
    NotifyStart(ExecutionUuid, WorkerUuid),
    /// The execution has completed with that result
    NotifyDone(ExecutionUuid, WorkerResult),
    /// The execution has been skipped
    NotifySkip(ExecutionUuid),
    /// There was an error during the evaluation
    Error(String),
    /// The server status as asked by the client
    Status(ExecutorStatus),
    /// The evaluation of the DAG is complete, this message will close the
    /// connection
    Done,
}

/// Messages sent by the workers to the server
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerClientMessage {
    /// The worker is ready for some job
    GetWork,
    /// The worker completed the job with this result
    WorkerDone(WorkerResult),
    /// The worker is sending a file to the server. After this message there
    /// is a protocol switch for the file transmission
    ProvideFile(FileUuid, FileStoreKey),
    /// The worker needs a file from the server
    AskFile(FileUuid),
}

/// Messages sent by the server to the worker
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerServerMessage {
    /// The job the worker should do. Boxed to reduce the enum size.
    Work(Box<WorkerJob>),
    /// The file the workers as asked After this message there is a protocol
    /// switch for the file transmission
    ProvideFile(FileUuid, FileStoreKey),
}

/// State of a worker as seen by the server
#[derive(Debug)]
pub struct WorkerState {
    /// Name of the worker
    pub name: String,
    /// Current job of the worker, None if the worker is waiting for some job
    pub job: Mutex<Option<WorkerJob>>,
    /// Conditional variable the worker thread on the server is waiting for
    /// some work to be ready
    pub cv: Condvar,
}

/// Internal state of the Executor
#[derive(Debug)]
pub struct ExecutorData {
    /// The current DAG which is being evaluated
    pub dag: Option<ExecutionDAGData>,
    /// The sets of callbaks the client is interested in
    pub callbacks: Option<ExecutionDAGCallbacks>,
    /// A channel to the client
    pub client_sender: Option<ChannelSender>,
    /// The state of the connected workers
    pub workers: HashMap<WorkerUuid, Arc<WorkerState>>,
    /// The priority queue of the ready tasks, waiting for the workers
    pub ready_execs: BinaryHeap<ExecutionUuid>,
    /// The list of tasks waiting for some dependencies, each value here is
    /// positive, when a task is ready it's removed from here
    pub missing_deps: HashMap<ExecutionUuid, usize>,
    /// The list of tasks that depends on a file, this is a lookup table for
    /// when the files become ready
    pub dependents: HashMap<FileUuid, Vec<ExecutionUuid>>,
    /// A reference to the server's FileStore
    pub file_store: Arc<Mutex<FileStore>>,
    /// The list of known FileStoreKeys for the current DAG
    pub file_keys: HashMap<FileUuid, FileStoreKey>,
}

/// The current status of the Executor, this is sent to the user when the
/// status is asked
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutorStatus {
    /// List of the connected workers with their name
    pub connected_workers: Vec<(WorkerUuid, String, bool)>,
    /// Number of running DAGs by the server
    pub running_dags: usize,
    /// Number of executions waiting for workers
    pub ready_execs: usize,
    /// Number of executions waiting for dependencies
    pub waiting_execs: usize,
}

/// The Executor is the main component of the server, this will receive the DAG
/// to evaluate and will schedule the tasks to the workers, sending to the
/// client the responses
pub struct Executor {
    /// The internals of the executor
    pub data: Arc<Mutex<ExecutorData>>,
}

impl Executor {
    /// Prepare a new Executor based on the specified FileStore
    pub fn new(file_store: Arc<Mutex<FileStore>>) -> Executor {
        Executor {
            data: Arc::new(Mutex::new(ExecutorData::new(file_store))),
        }
    }

    /// Connect a new worker to the server, this will spawn a new thread that
    /// will manage the connection with the worker
    pub fn add_worker(&mut self, worker: WorkerConn) {
        let data = self.data.clone();
        thread::Builder::new()
            .name(format!("Executor worker thread for {}", worker))
            .spawn(move || {
                worker_thread(data, worker).expect("Worker failed");
            })
            .expect("Failed to spawn executor worker thread");
    }

    /// Starts the Executor for a client, this will block and will manage the
    /// communication with the client.
    ///
    /// * `sender` - A channel that sends messages to the client
    /// * `receiver` - A channel that receives messages from the client
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
                        drop(receiver);
                        break;
                    } else {
                        trace!("DAG looks valid!");
                    }
                    {
                        let mut data = self.data.lock().unwrap();
                        data.dag = Some(dag);
                        data.callbacks = Some(callbacks);
                        data.client_sender = Some(sender.clone());
                    }
                    Scheduler::setup(self.data.clone());
                    Scheduler::schedule(self.data.clone());
                    let ready_files = {
                        let mut data = self.data.lock().unwrap();
                        let mut ready_files = vec![];
                        let provided_files = data.dag.as_ref().unwrap().provided_files.clone();
                        for (uuid, file) in provided_files.into_iter() {
                            if !data.file_store.lock().unwrap().has_key(&file.key) {
                                serialize_into(&ExecutorServerMessage::AskFile(uuid), &sender)?;
                            } else {
                                data.file_store.lock().unwrap().persist(&file.key)?;
                                data.file_keys.insert(uuid, file.key.clone());
                                ready_files.push(uuid);
                                trace!("File {} already in store!", uuid);
                            }
                        }
                        ready_files
                    };
                    for file in ready_files.into_iter() {
                        Scheduler::file_ready(self.data.clone(), file);
                    }
                }
                Ok(ExecutorClientMessage::ProvideFile(uuid, key)) => {
                    info!("Client sent: {} {:?}", uuid, key);
                    {
                        let mut data = self.data.lock().unwrap();
                        if data.dag.is_none() {
                            warn!("Provided file before the DAG!");
                            drop(receiver);
                            break;
                        }
                        data.file_store
                            .lock()
                            .unwrap()
                            .store(&key, ChannelFileIterator::new(&receiver))?;
                        data.file_keys.insert(uuid, key.clone());
                    }
                    Scheduler::file_ready(self.data.clone(), uuid);
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
                                        worker.job.lock().unwrap().is_some(),
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
                    drop(receiver);
                    // TODO stop all the workers
                    break;
                }
                Err(e) => {
                    // TODO stop all the workers
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
        Ok(())
    }
}

impl ExecutorData {
    /// Make a new ExecutorData based on the specified FileStore
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
        }
    }
}

/// Thread function that manages the connection with a worker. This function is
/// intended to be called in a thread, this will block the thread until the
/// worker disconnects.
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
                    executor.workers.insert(
                        conn.uuid,
                        Arc::new(WorkerState {
                            name: conn.name.clone(),
                            job: Mutex::new(None),
                            cv: Condvar::new(),
                        }),
                    );
                }

                Scheduler::schedule(executor.clone());
                let job = wait_for_work(executor.clone(), &conn.uuid);
                serialize_into(&WorkerServerMessage::Work(Box::new(job)), &conn.sender).unwrap();
            }
            Ok(WorkerClientMessage::WorkerDone(result)) => {
                info!("Worker {} completed with: {:?}", conn, result);
                let exec_uuid = {
                    let mut data = executor.lock().unwrap();
                    let exec = data
                        .workers
                        .get(&conn.uuid)
                        .expect("Worker disappeared")
                        .job
                        .lock()
                        .unwrap()
                        .clone();
                    assert!(exec.is_some(), "Worker job disappeared");
                    let exec_uuid = exec.unwrap().clone().execution.uuid;
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
                    exec_uuid
                };
                match result.result.status {
                    ExecutionStatus::Success => {}
                    _ => Scheduler::exec_failed(executor.clone(), exec_uuid),
                }
            }
            Ok(WorkerClientMessage::ProvideFile(uuid, key)) => {
                info!("Worker provided file {} {:?}", uuid, key);
                {
                    let mut data = executor.lock().unwrap();
                    data.file_store
                        .lock()
                        .unwrap()
                        .store(&key, ChannelFileIterator::new(&conn.receiver))?;
                    data.file_keys.insert(uuid, key.clone());
                }
                Scheduler::file_ready(executor.clone(), uuid);
                let data = executor.lock().unwrap();
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
                    executor.lock().unwrap().workers.remove(&conn.uuid);
                    info!("Removed worker {} from pool", conn);
                    Scheduler::schedule(executor.clone());
                    break;
                } else {
                    error!("Connection error: {}", cause);
                }
            }
        }
    }
    Ok(())
}

/// Block the thread until there is something to do for this worker
fn wait_for_work(executor: Arc<Mutex<ExecutorData>>, uuid: &WorkerUuid) -> WorkerJob {
    let worker = &*executor.lock().unwrap().workers[&uuid].clone();
    let mut job = worker.job.lock().unwrap();
    while job.is_none() {
        job = worker.cv.wait(job).unwrap();
    }
    job.clone().unwrap()
}
