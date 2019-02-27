use crate::execution::*;
use crate::executor::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use uuid::Uuid;

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
    Work(String),
}

struct ExecutorData {
    dag: Option<ExecutionDAGData>,
}

pub struct Executor {
    data: Arc<Mutex<ExecutorData>>,
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
        info!("Executor started");
        loop {
            let message = deserialize_from::<ExecutorClientMessage>(&receiver);
            match message {
                Ok(ExecutorClientMessage::Evaluate(d)) => {
                    info!("Want to evaluate a DAG!");
                    self.data.lock().unwrap().dag = Some(d);
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
        ExecutorData { dag: None }
    }
}

fn worker_thread(executor: Arc<Mutex<ExecutorData>>, conn: WorkerConn) {
    info!("Server connected to worker {}", conn);
    loop {
        let message = deserialize_from::<WorkerClientMessage>(&conn.receiver);
        match message {
            Ok(WorkerClientMessage::GetWork) => {
                info!("Worker {} ready for work", conn);
            }
            Ok(WorkerClientMessage::WorkerError(error)) => {
                info!("Worker {} failed with error: {}", conn, error);
            }
            Ok(WorkerClientMessage::WorkerSuccess(result)) => {
                info!("Worker {} succeded with: {}", conn, result);
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
}
