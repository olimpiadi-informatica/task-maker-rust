use crate::execution::*;
use crate::executor::*;
use crate::store::*;
use failure::{Error, Fail};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use uuid::Uuid;

pub type WorkerUuid = Uuid;

pub struct Worker {
    pub uuid: WorkerUuid,
    pub name: String,
    pub sender: Sender<String>,
    pub receiver: Receiver<String>,
    pub file_store: Arc<Mutex<FileStore>>,
}

pub struct WorkerConn {
    pub uuid: WorkerUuid,
    pub name: String,
    pub sender: Sender<String>,
    pub receiver: Receiver<String>,
}

#[derive(Debug, Fail)]
pub enum WorkerError {
    #[fail(display = "missing key for dependency {}", uuid)]
    MissingDependencyKey { uuid: Uuid },
}

impl Worker {
    pub fn new(name: &str, file_store: Arc<Mutex<FileStore>>) -> (Worker, WorkerConn) {
        let (tx, rx_worker) = channel();
        let (tx_worker, rx) = channel();
        let uuid = Uuid::new_v4();
        (
            Worker {
                uuid: uuid.clone(),
                name: name.to_owned(),
                sender: tx_worker,
                receiver: rx_worker,
                file_store: file_store,
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
        trace!("Worker {} ready, asking for work", self);
        serialize_into(&WorkerClientMessage::GetWork, &self.sender)?;
        loop {
            let message = deserialize_from::<WorkerServerMessage>(&self.receiver);
            match message {
                Ok(WorkerServerMessage::Work(job)) => {
                    trace!("Worker {} got job: {:?}", self, job);
                    for input in job.execution.dependencies().iter() {
                        let store = self.file_store.lock().unwrap();
                        let key =
                            job.dep_keys
                                .get(&input)
                                .ok_or(WorkerError::MissingDependencyKey {
                                    uuid: input.clone(),
                                })?;
                        if !store.has_key(&key) {
                            serialize_into(
                                &WorkerClientMessage::AskFile(input.clone()),
                                &self.sender,
                            )?;
                        }
                    }
                    thread::sleep(std::time::Duration::from_secs(1));
                    serialize_into(
                        &WorkerClientMessage::WorkerDone(WorkerResult {
                            result: ExecutionResult {
                                uuid: job.execution.uuid.clone(),
                                status: ExecutionStatus::Success,
                            },
                        }),
                        &self.sender,
                    )?;
                    for out in job.execution.outputs() {
                        let path = std::path::Path::new("/dev/null");
                        serialize_into(
                            &WorkerClientMessage::ProvideFile(
                                out.clone(),
                                FileStoreKey::from_file(path)?,
                            ),
                            &self.sender,
                        )
                        .unwrap();
                        ChannelFileSender::send(path, &self.sender)?;
                    }
                    serialize_into(&WorkerClientMessage::GetWork, &self.sender)?;
                }
                Ok(WorkerServerMessage::ProvideFile(uuid, key)) => {
                    info!("Server sent file {} {:?}", uuid, key);
                    let mut store = self.file_store.lock().unwrap();
                    let reader = ChannelFileIterator::new(&self.receiver);
                    store.store(&key, reader)?;
                }
                Err(e) => {
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
