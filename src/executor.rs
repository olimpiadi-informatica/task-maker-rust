use crate::execution::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
enum ExecutorClientMessage {
    Evaluate(ExecutionDAG),
    Test(i32),
}

struct WorkerConn {
    uuid: Uuid,
    name: String,
    sender: Sender<String>,
    receiver: Receiver<String>,
}

struct Executor {
    workers: Vec<WorkerConn>,
}

pub struct LocalExecutor {
    executor: Executor,
    num_workers: usize,
}

pub struct ExecutorClient;

pub trait ExecutorTrait {
    fn evaluate(&mut self, sender: Sender<String>, receiver: Receiver<String>)
        -> Result<(), Error>;
}

impl Executor {
    pub fn new() -> Executor {
        Executor { workers: vec![] }
    }

    fn add_worker(&mut self, worker: WorkerConn) {
        self.workers.push(worker);
    }

    fn evaluate(
        &mut self,
        sender: Sender<String>,
        receiver: Receiver<String>,
    ) -> Result<(), Error> {
        let data = deserialize_from::<ExecutorClientMessage>(&receiver)?;
        serialize_into(&data, &self.workers.first().unwrap().sender)?;
        println!("Recv: {:?}", self.workers.first().unwrap().receiver.recv());
        // TODO implement server
        Ok(())
    }
}

impl LocalExecutor {
    pub fn new(num_workers: usize) -> LocalExecutor {
        let mut executor = Executor::new();
        println!("Spawning {} workers", num_workers);
        for i in 0..num_workers {
            println!("Spawning worker {}", i);
            let (tx, rx_worker) = channel();
            let (tx_worker, rx) = channel();
            executor.add_worker(WorkerConn::new(&format!("Local worker {}", i), tx, rx));
            thread::spawn(move || {
                println!("I'm worker {}", i);
                loop {
                    // let data = rx_worker.recv();
                    let data = deserialize_from::<ExecutorClientMessage>(&rx_worker);
                    println!("[Worker{}] {:?}", i, data);
                    if data.is_err() {
                        break;
                    }
                    tx_worker.send("okok".to_owned()).unwrap();
                    // TODO implement worker
                }
            });
        }
        LocalExecutor {
            executor,
            num_workers,
        }
    }
}

impl ExecutorTrait for LocalExecutor {
    fn evaluate(
        &mut self,
        sender: Sender<String>,
        receiver: Receiver<String>,
    ) -> Result<(), Error> {
        self.executor.evaluate(sender, receiver)
    }
}

impl ExecutorClient {
    pub fn evaluate(
        dag: ExecutionDAG,
        sender: Sender<String>,
        receiver: Receiver<String>,
    ) -> Result<(), Error> {
        serialize_into(&ExecutorClientMessage::Evaluate(dag), &sender)?;
        println!("Message sent");
        Ok(())
    }
}

fn serialize_into<T>(what: &T, sender: &Sender<String>) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender
        .send(serde_json::to_string(what)?)
        .map_err(|e| e.into())
}

fn deserialize_from<T>(reader: &Receiver<String>) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

impl WorkerConn {
    pub fn new(name: &str, sender: Sender<String>, receiver: Receiver<String>) -> WorkerConn {
        WorkerConn {
            uuid: Uuid::new_v4(),
            name: name.to_owned(),
            sender,
            receiver,
        }
    }
}
