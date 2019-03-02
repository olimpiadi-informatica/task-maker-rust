use crate::execution::*;
use crate::executor::scheduler::Scheduler;
use crate::executor::*;
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
    ProvideFile(FileUuid),
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
    Status(String),
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
    pub ready_execs: BinaryHeap<ExecutionUuid>,
    pub missing_deps: HashMap<ExecutionUuid, usize>,
    pub dependents: HashMap<FileUuid, Vec<ExecutionUuid>>,
}

pub struct Executor {
    pub data: Arc<Mutex<ExecutorData>>,
}

pub trait ExecutorTrait {
    fn evaluate(&mut self, sender: Sender<String>, receiver: Receiver<String>)
        -> Result<(), Error>;
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
                    let files: Vec<FileUuid> =
                        dag.provided_files.keys().map(|k| k.clone()).collect();
                    {
                        let mut data = self.data.lock().unwrap();
                        data.dag = Some(dag);
                        data.callbacks = Some(callbacks);
                        data.client_sender = Some(sender.clone());
                    }
                    Scheduler::setup(self.data.clone());
                    // TODO: this is just a mock
                    for file in files.iter() {
                        Scheduler::file_ready(self.data.clone(), *file);
                    }
                    Scheduler::schedule(self.data.clone());
                }
                Ok(ExecutorClientMessage::ProvideFile(uuid)) => {
                    info!("Client sent: {}", uuid);
                    Scheduler::schedule(self.data.clone());
                    break;
                }
                Ok(ExecutorClientMessage::Status) => {
                    info!("Client asking for the status");
                    // TODO real status
                    serialize_into(
                        &ExecutorServerMessage::Status("Good, thanks".to_owned()),
                        &sender,
                    )?;
                }
                Ok(ExecutorClientMessage::Stop) => {
                    drop(receiver);
                    break;
                }
                Err(e) => {
                    let cause = e.find_root_cause().to_string();
                    info!("Connection error: {}", cause);
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
                executor.lock().unwrap().waiting_workers.insert(
                    conn.uuid.clone(),
                    Arc::new((Mutex::new(None), Condvar::new())),
                );
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
                info!("Connection error: {}", cause);
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
