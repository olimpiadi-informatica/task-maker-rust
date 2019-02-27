use crate::executor::*;
use std::sync::mpsc::{channel, Receiver, Sender};
use uuid::Uuid;

pub struct Worker {
    pub uuid: Uuid,
    pub name: String,
    pub sender: Sender<String>,
    pub receiver: Receiver<String>,
}

pub struct WorkerConn {
    pub uuid: Uuid,
    pub name: String,
    pub sender: Sender<String>,
    pub receiver: Receiver<String>,
}

impl Worker {
    pub fn new(name: &str) -> (Worker, WorkerConn) {
        let (tx, rx_worker) = channel();
        let (tx_worker, rx) = channel();
        let uuid = Uuid::new_v4();
        (
            Worker {
                uuid: uuid.clone(),
                name: name.to_owned(),
                sender: tx_worker,
                receiver: rx_worker,
            },
            WorkerConn {
                uuid: uuid,
                name: name.to_owned(),
                sender: tx,
                receiver: rx,
            },
        )
    }

    pub fn work(self) -> Result<(), Error> {
        info!("Worker {} ready", self);
        serialize_into(&WorkerClientMessage::GetWork, &self.sender)?;
        loop {
            let message = deserialize_from::<WorkerServerMessage>(&self.receiver);
            match message {
                Ok(WorkerServerMessage::Work(what)) => {
                    info!("Worker {} got job: {}", self, what);
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

impl std::fmt::Display for WorkerConn {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "'{}' ({})", self.name, self.uuid)
    }
}

impl std::fmt::Display for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "'{}' ({})", self.name, self.uuid)
    }
}
