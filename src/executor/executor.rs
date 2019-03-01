use crate::execution::*;
use crate::executor::scheduler::schedule;
use crate::executor::*;
use failure::{Error, Fail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use uuid::Uuid;

pub type Work = String;

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorClientMessage {
    Evaluate(ExecutionDAGData),
    ProvideFile(Uuid),
    Stop,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorServerMessage {
    AskFile(Uuid),
    NotifyStart(Uuid, Uuid),
    NotifyDone(Uuid, String),
    NotifySkip(Uuid),
    Error(String),
    Status(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerClientMessage {
    GetWork,
    WorkerSuccess(String),
    WorkerError(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerServerMessage {
    Work(Work),
}

#[derive(Debug)]
pub struct ExecutorData {
    pub dag: Option<ExecutionDAGData>,
    pub waiting_workers: HashMap<Uuid, Arc<(Mutex<Option<Work>>, Condvar)>>,
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
        self.data
            .lock()
            .unwrap()
            .waiting_workers
            .insert(worker.uuid, Arc::new((Mutex::new(None), Condvar::new())));
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
                Ok(ExecutorClientMessage::Evaluate(d)) => {
                    info!("Want to evaluate a DAG!");
                    if let Err(e) = check_dag(&d) {
                        warn!("Invalid DAG: {:?}", e);
                        serialize_into(&ExecutorServerMessage::Error(e.to_string()), &sender)?;
                        drop(receiver);
                        break;
                    } else {
                        info!("DAG looks valid!");
                    }
                    self.data.lock().unwrap().dag = Some(d);
                    schedule(self.data.clone());
                }
                Ok(ExecutorClientMessage::ProvideFile(uuid)) => {
                    info!("Client sent: {}", uuid);
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
            waiting_workers: HashMap::new(),
        }
    }
}

fn worker_thread(executor: Arc<Mutex<ExecutorData>>, conn: WorkerConn) {
    info!("Server connected to worker {}", conn);
    loop {
        let message = deserialize_from::<WorkerClientMessage>(&conn.receiver);
        match message {
            Ok(WorkerClientMessage::GetWork) => {
                info!("Worker {} ready for work", conn);
                schedule(executor.clone());
                let job = wait_for_work(executor.clone(), &conn.uuid);
                serialize_into(&WorkerServerMessage::Work(job), &conn.sender).unwrap();
            }
            Ok(WorkerClientMessage::WorkerError(error)) => {
                info!("Worker {} failed with error: {}", conn, error);
                // TODO update job state
                schedule(executor.clone());
            }
            Ok(WorkerClientMessage::WorkerSuccess(result)) => {
                info!("Worker {} succeded with: {}", conn, result);
                // TODO update job state
                schedule(executor.clone());
            }
            Err(e) => {
                let cause = e.find_root_cause().to_string();
                info!("Connection error: {}", cause);
                if cause == "receiving on a closed channel" {
                    executor.lock().unwrap().waiting_workers.remove(&conn.uuid);
                    info!("Removed worker {} from pool", conn);
                    schedule(executor.clone());
                    break;
                }
            }
        }
    }
}

fn wait_for_work(executor: Arc<Mutex<ExecutorData>>, uuid: &Uuid) -> Work {
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
    let content = job.clone().unwrap();
    *job = None;
    content
}
