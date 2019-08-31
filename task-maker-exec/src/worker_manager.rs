use crate::proto::{
    ChannelFileIterator, ChannelFileSender, WorkerClientMessage, WorkerServerMessage,
};
use crate::SchedulerInMessage;
use crate::{deserialize_from, serialize_into, ChannelSender, WorkerConn};
use failure::{format_err, Error};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use task_maker_dag::WorkerUuid;
use task_maker_store::FileStore;

/// The entity that manage the connections with the workers, eventually writing files to disk and
/// talking to the `Scheduler`.
pub(crate) struct WorkerManager {
    /// The list of all the workers that are currently connected to the manager.
    connected_workers: HashMap<WorkerUuid, ChannelSender>,
    /// A reference to the `FileStore`.
    file_store: Arc<FileStore>,
    /// The channel to use to send messages to the `Scheduler`.
    scheduler: Sender<SchedulerInMessage>,
}

impl WorkerManager {
    /// Make a new `WorkerManager` bound to the specified `Scheduler`.
    pub fn new(file_store: Arc<FileStore>, scheduler: Sender<SchedulerInMessage>) -> WorkerManager {
        WorkerManager {
            connected_workers: HashMap::new(),
            file_store,
            scheduler,
        }
    }

    /// Add a worker to the list, spawning a new thread for managing it and returning the handle for
    /// joining it.
    pub fn add(&mut self, worker: WorkerConn) -> JoinHandle<()> {
        self.connected_workers
            .insert(worker.uuid, worker.sender.clone());
        let scheduler = self.scheduler.clone();
        let file_store = self.file_store.clone();
        std::thread::Builder::new()
            .name(format!(
                "Manager of worker {} ({})",
                worker.name, worker.uuid
            ))
            .spawn(move || WorkerManager::worker_thread(worker, scheduler, file_store).unwrap())
            .expect("Failed to spawn manager of worker")
    }

    /// Stop all the workers by sending to them the `Exit` command and dropping the sender.
    pub fn stop(&mut self) -> Result<(), Error> {
        for (_, sender) in self.connected_workers.drain() {
            serialize_into(&WorkerServerMessage::Exit, &sender)?;
        }
        Ok(())
    }

    /// Body of the thread that manages the connection to a worker.
    fn worker_thread(
        worker: WorkerConn,
        scheduler: Sender<SchedulerInMessage>,
        file_store: Arc<FileStore>,
    ) -> Result<(), Error> {
        loop {
            let message = deserialize_from::<WorkerClientMessage>(&worker.receiver);
            match message {
                Ok(WorkerClientMessage::GetWork) => {
                    if scheduler
                        .send(SchedulerInMessage::WorkerConnected {
                            uuid: worker.uuid,
                            name: worker.name.clone(),
                            sender: worker.sender.clone(),
                        })
                        .is_err()
                    {
                        // the scheduler is gone
                        break;
                    }
                }
                Ok(WorkerClientMessage::AskFile(key)) => {
                    let handle = file_store
                        .get(&key)
                        .expect("Worker is asking for an unknown file");
                    serialize_into(&WorkerServerMessage::ProvideFile(key), &worker.sender)?;
                    ChannelFileSender::send(handle.path(), &worker.sender)?;
                }
                Ok(WorkerClientMessage::ProvideFile(_, _)) => {
                    unreachable!("Unexpected ProvideFile from worker");
                }
                Ok(WorkerClientMessage::WorkerDone(result, outputs)) => {
                    let mut output_handlers = HashMap::new();
                    for _ in 0..outputs.len() {
                        let message = deserialize_from::<WorkerClientMessage>(&worker.receiver)?;
                        if let WorkerClientMessage::ProvideFile(uuid, key) = message {
                            let handle = file_store
                                .store(&key, ChannelFileIterator::new(&worker.receiver))?;
                            output_handlers.insert(uuid, handle);
                        } else {
                            panic!("Unexpected message from worker: {:?}", message);
                        }
                    }
                    scheduler
                        .send(SchedulerInMessage::WorkerResult {
                            worker: worker.uuid,
                            result,
                            outputs: output_handlers,
                        })
                        .map_err(|e| format_err!("Failed to send message to scheduler: {:?}", e))?;;
                }
                Err(_) => {
                    if scheduler
                        .send(SchedulerInMessage::WorkerDisconnected { uuid: worker.uuid })
                        .is_err()
                    {
                        debug!("Cannot tell the scheduler that a worker left, maybe it's gone");
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}
