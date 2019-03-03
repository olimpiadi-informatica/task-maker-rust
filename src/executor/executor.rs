use crate::execution::*;
use crate::executor::scheduler::Scheduler;
use crate::executor::*;
use crate::store::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerJob {
    pub execution: Execution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    pub result: ExecutionResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorClientMessage {
    Evaluate {
        dag: ExecutionDAGData,
        callbacks: ExecutionDAGCallbacks,
    },
    ProvideFile(FileUuid, FileStoreKey),
    Stop,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorServerMessage {
    AskFile(FileUuid),
    ProvideFile(FileUuid),
    NotifyStart(ExecutionUuid, WorkerUuid),
    NotifyDone(ExecutionUuid, WorkerResult),
    NotifySkip(ExecutionUuid),
    Error(String),
    Status(ExecutorStatus),
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerClientMessage {
    GetWork,
    WorkerDone(WorkerResult),
    ProvideFile(FileUuid),
    AskFile(FileUuid),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerServerMessage {
    Work(WorkerJob),
    ProvideFile(FileUuid),
}

#[derive(Debug)]
pub struct ExecutorData {
    pub dag: Option<ExecutionDAGData>,
    pub callbacks: Option<ExecutionDAGCallbacks>,
    pub client_sender: Option<Sender<String>>,
    pub waiting_workers: HashMap<WorkerUuid, Arc<(Mutex<Option<WorkerJob>>, Condvar)>>,
    pub worker_names: HashMap<WorkerUuid, String>,
    pub ready_execs: BinaryHeap<ExecutionUuid>,
    pub missing_deps: HashMap<ExecutionUuid, usize>,
    pub dependents: HashMap<FileUuid, Vec<ExecutionUuid>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutorStatus {
    pub connected_workers: Vec<(WorkerUuid, String, bool)>,
    pub running_dags: usize,
    pub ready_execs: usize,
    pub waiting_execs: usize,
}

pub struct Executor {
    pub data: Arc<Mutex<ExecutorData>>,
}

pub trait ExecutorTrait {
    fn evaluate(
        &mut self,
        file_store: FileStore,
        sender: Sender<String>,
        receiver: Receiver<String>,
    ) -> Result<(), Error>;
}

impl Executor {
    pub fn new() -> Executor {
        Executor {
            data: Arc::new(Mutex::new(ExecutorData::new())),
        }
    }

    pub fn add_worker(&mut self, worker: WorkerConn) {
        let data = self.data.clone();
        thread::Builder::new()
            .name(format!("Executor worker thread for {}", worker))
            .spawn(move || {
                worker_thread(data, worker);
            })
            .expect("Failed to spawn executor worker thread");
    }

    pub fn evaluate(
        &mut self,
        mut file_store: FileStore,
        sender: Sender<String>,
        receiver: Receiver<String>,
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
                        info!("DAG looks valid!");
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
                        let data = self.data.lock().unwrap();
                        let mut ready_files = vec![];
                        for (uuid, file) in data.dag.as_ref().unwrap().provided_files.iter() {
                            if !file_store.has_key(&file.key) {
                                serialize_into(
                                    &ExecutorServerMessage::AskFile(uuid.clone()),
                                    &sender,
                                )?;
                            } else {
                                file_store.persist(&file.key)?;
                                ready_files.push(uuid.clone());
                                info!("File {} already in store!", uuid);
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
                    if self.data.lock().unwrap().dag.is_none() {
                        warn!("Provided file before the DAG!");
                        drop(receiver);
                        break;
                    }
                    file_store.store(&key, ChannelFileIterator::new(&receiver))?;
                    Scheduler::file_ready(self.data.clone(), uuid);
                }
                Ok(ExecutorClientMessage::Status) => {
                    info!("Client asking for the status");
                    let data = self.data.lock().unwrap();
                    serialize_into(
                        &ExecutorServerMessage::Status(ExecutorStatus {
                            connected_workers: data
                                .waiting_workers
                                .iter()
                                .map(|(uuid, job)| {
                                    (
                                        uuid.clone(),
                                        data.worker_names
                                            .get(&uuid)
                                            .unwrap_or(&"unknown".to_owned())
                                            .clone(),
                                        job.0.lock().unwrap().is_some(),
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
                    break;
                }
                Err(e) => {
                    // TODO stop all the workers
                    let cause = e.find_root_cause().to_string();
                    trace!("Connection error: {}", cause);
                    if cause == "receiving on a closed channel" {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl ExecutorData {
    fn new() -> ExecutorData {
        ExecutorData {
            dag: None,
            callbacks: None,
            client_sender: None,
            waiting_workers: HashMap::new(),
            worker_names: HashMap::new(),
            ready_execs: BinaryHeap::new(),
            missing_deps: HashMap::new(),
            dependents: HashMap::new(),
        }
    }
}

fn worker_thread(executor: Arc<Mutex<ExecutorData>>, conn: WorkerConn) {
    trace!("Server connected to worker {}", conn);

    loop {
        let message = deserialize_from::<WorkerClientMessage>(&conn.receiver);
        match message {
            Ok(WorkerClientMessage::GetWork) => {
                trace!("Worker {} ready for work", conn);
                assert!(!executor
                    .lock()
                    .unwrap()
                    .waiting_workers
                    .contains_key(&conn.uuid));
                {
                    let mut executor = executor.lock().unwrap();
                    executor.waiting_workers.insert(
                        conn.uuid.clone(),
                        Arc::new((Mutex::new(None), Condvar::new())),
                    );
                    executor
                        .worker_names
                        .insert(conn.uuid.clone(), conn.name.clone());
                }

                Scheduler::schedule(executor.clone());
                let job = wait_for_work(executor.clone(), &conn.uuid);
                serialize_into(&WorkerServerMessage::Work(job), &conn.sender).unwrap();
            }
            Ok(WorkerClientMessage::WorkerDone(result)) => {
                info!("Worker {} completed with: {:?}", conn, result);
                let exec_uuid = {
                    let mut data = executor.lock().unwrap();
                    let exec = data
                        .waiting_workers
                        .get(&conn.uuid)
                        .expect("Worker disappeared")
                        .0
                        .lock()
                        .unwrap()
                        .clone();
                    assert!(exec.is_some(), "Worker job disappeared");
                    let exec_uuid = exec.unwrap().clone().execution.uuid;
                    data.waiting_workers.remove(&conn.uuid);
                    data.worker_names.remove(&conn.uuid);
                    if data
                        .callbacks
                        .as_ref()
                        .unwrap()
                        .executions
                        .contains(&exec_uuid)
                    {
                        serialize_into(
                            &ExecutorServerMessage::NotifyDone(exec_uuid.clone(), result.clone()),
                            data.client_sender.as_ref().unwrap(),
                        )
                        .expect("Cannot send message to client");
                    }
                    exec_uuid
                };
                match result.result.status {
                    ExecutionStatus::Success => {}
                    _ => Scheduler::exec_failed(executor.clone(), exec_uuid),
                }
            }
            Ok(WorkerClientMessage::ProvideFile(uuid)) => {
                info!("Worker provided file {}", uuid);
                Scheduler::file_ready(executor.clone(), uuid);
                let data = executor.lock().unwrap();
                if data.callbacks.as_ref().unwrap().files.contains(&uuid) {
                    serialize_into(
                        &ExecutorServerMessage::ProvideFile(uuid),
                        &data.client_sender.as_ref().unwrap(),
                    )
                    .expect("Cannot send message to client");
                }
            }
            Ok(WorkerClientMessage::AskFile(uuid)) => {
                serialize_into(&ExecutorServerMessage::ProvideFile(uuid), &conn.sender).unwrap();
            }
            Err(e) => {
                let cause = e.find_root_cause().to_string();
                trace!("Connection error: {}", cause);
                if cause == "receiving on a closed channel" {
                    executor.lock().unwrap().waiting_workers.remove(&conn.uuid);
                    info!("Removed worker {} from pool", conn);
                    Scheduler::schedule(executor.clone());
                    break;
                }
            }
        }
    }
}

fn wait_for_work(executor: Arc<Mutex<ExecutorData>>, uuid: &WorkerUuid) -> WorkerJob {
    let (lock, cv) = &*executor
        .lock()
        .unwrap()
        .waiting_workers
        .get(&uuid)
        .unwrap()
        .clone();
    let mut job = lock.lock().unwrap();
    while job.is_none() {
        job = cv.wait(job).unwrap();
    }
    job.clone().unwrap()
}
