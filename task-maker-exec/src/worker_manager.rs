use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;

use failure::Error;

use task_maker_dag::{ExecutionGroupUuid, WorkerUuid};
use task_maker_store::FileStore;

use crate::executor::WorkerJob;
use crate::proto::{
    ChannelFileIterator, ChannelFileSender, WorkerClientMessage, WorkerServerMessage,
};
use crate::scheduler::SchedulerInMessage;
use crate::{ChannelSender, WorkerConn};

/// Message coming from the Scheduler or the Executor for the WorkerManager
#[allow(clippy::large_enum_variant)]
pub(crate) enum WorkerManagerInMessage {
    /// A new worker has connected. The WorkerManager will take care of it.
    WorkerConnected { worker: WorkerConn },
    /// A worker has disconnected. This message is sent by the WorkerManager itself, from a
    /// different thread.
    WorkerDisconnected { worker: WorkerUuid },
    /// The scheduler sent a new job for a worker. The WorkerManager will forward the job to the
    /// actual worker.
    WorkerJob { worker: WorkerUuid, job: WorkerJob },
    /// The scheduler is asking a worker to stop doing a job, if the worker is still working on
    /// that.
    StopWorkerJob {
        worker: WorkerUuid,
        job: ExecutionGroupUuid,
    },
    /// The WorkerManager is asked to exit and tell all the connected worker to exit too.
    Exit,
}

/// The entity that manages the connections with the workers, eventually writing files to disk and
/// telling to the `Scheduler` the connection and disconnection of the workers.
pub(crate) struct WorkerManager {
    /// A reference to the file store.
    file_store: Arc<FileStore>,
    /// A channel for sending the messages to the scheduler.
    scheduler: Sender<SchedulerInMessage>,
    /// A channel for sending the messages to the WorkerManager itself. It is used by the threads
    /// that manage the actual workers for sending back the notification of disconnection.
    sender: Sender<WorkerManagerInMessage>,
    /// The receiver of the messages for the worker manager.
    receiver: Receiver<WorkerManagerInMessage>,
}

impl WorkerManager {
    /// Make a new `WorkerManager` based on the specified file store, talking to the specified
    /// scheduler. `sender` is just a sender that sends messages to the `receiver`, this is needed
    /// internally for sending back the disconnection notification from other threads.
    pub fn new(
        file_store: Arc<FileStore>,
        scheduler: Sender<SchedulerInMessage>,
        sender: Sender<WorkerManagerInMessage>,
        receiver: Receiver<WorkerManagerInMessage>,
    ) -> WorkerManager {
        WorkerManager {
            file_store,
            scheduler,
            sender,
            receiver,
        }
    }

    /// Run the worker manager blocking until an exit message is received. On exiting the connected
    /// workers will stop.
    pub fn run(self) -> Result<(), Error> {
        let mut connected_workers: HashMap<WorkerUuid, ChannelSender<WorkerServerMessage>> =
            HashMap::new();
        while let Ok(message) = self.receiver.recv() {
            match message {
                WorkerManagerInMessage::WorkerConnected { worker } => {
                    if connected_workers.contains_key(&worker.uuid) {
                        warn!("Duplicate worker uuid");
                        continue;
                    }
                    connected_workers.insert(worker.uuid, worker.sender.clone());
                    info!("Worker {} ({}) connected", worker.name, worker.uuid);
                    let scheduler = self.scheduler.clone();
                    let file_store = self.file_store.clone();
                    let sender = self.sender.clone();
                    thread::Builder::new()
                        .name(format!(
                            "Manager of worker {} ({})",
                            worker.name, worker.uuid
                        ))
                        .spawn(move || {
                            if let Err(e) =
                                WorkerManager::worker_thread(worker, scheduler, sender, file_store)
                            {
                                warn!("The manager of a worker failed: {:?}", e);
                            }
                        })
                        .expect("Failed to spawn manager for a worker");
                }
                WorkerManagerInMessage::WorkerDisconnected { worker } => {
                    connected_workers
                        .remove(&worker)
                        .expect("Unknown worker disconnected");
                }
                WorkerManagerInMessage::WorkerJob { worker, job } => {
                    // if the worker is not present, it means it has just disconnected. The
                    // scheduler should be already informed and should have resheduled the job.
                    if let Some(sender) = connected_workers.get(&worker) {
                        sender.send(WorkerServerMessage::Work(Box::new(job)))?;
                    }
                }
                WorkerManagerInMessage::Exit => {
                    debug!("Worker manager asked to exit");
                    break;
                }
                WorkerManagerInMessage::StopWorkerJob { worker, job } => {
                    if let Some(sender) = connected_workers.get(&worker) {
                        sender.send(WorkerServerMessage::KillJob(job))?;
                    }
                }
            }
        }
        debug!("Worker manager exiting");
        for (worker, sender) in connected_workers.iter() {
            if sender.send(WorkerServerMessage::Exit).is_err() {
                warn!("Cannot tell worker {} to exit", worker);
            }
        }
        Ok(())
    }

    /// Thread body that manages the actual connection with a worker. `worker_manager` will send
    /// messages back to the `WorkerManager` main thread for the notification about the
    /// disconnection of this worker.
    fn worker_thread(
        worker: WorkerConn,
        scheduler: Sender<SchedulerInMessage>,
        worker_manager: Sender<WorkerManagerInMessage>,
        file_store: Arc<FileStore>,
    ) -> Result<(), Error> {
        while let Ok(message) = worker.receiver.recv() {
            match message {
                WorkerClientMessage::GetWork => {
                    // the worker is asking for more work to do
                    let res = scheduler.send(SchedulerInMessage::WorkerConnected {
                        uuid: worker.uuid,
                        name: worker.name.clone(),
                    });
                    if res.is_err() {
                        // the scheduler is gone
                        break;
                    }
                }
                WorkerClientMessage::AskFile(key) => {
                    // the worker is asking for a file it doesn't have locally stored
                    let handle = file_store
                        .get(&key)
                        .expect("Worker is asking for an unknown file");
                    worker.sender.send(WorkerServerMessage::ProvideFile(key))?;
                    ChannelFileSender::send(handle.path(), &worker.sender)?;
                }
                WorkerClientMessage::ProvideFile(_, _) => {
                    // the worker should not provide files unless just after a WorkerDone message is
                    // received
                    unreachable!("Unexpected ProvideFile from worker");
                }
                WorkerClientMessage::WorkerDone(result, outputs) => {
                    // the worker completed its job and will send the produced files
                    let mut output_handlers = HashMap::new();
                    let mut missing_files = Vec::new();
                    for (uuid, key) in &outputs {
                        if let Some(handle) = file_store.get(key) {
                            output_handlers.insert(*uuid, handle);
                        } else {
                            missing_files.push(*uuid);
                        }
                    }
                    let num_missing = missing_files.len();
                    info!(
                        "Asking worker {} for {} missing files",
                        worker.uuid, num_missing
                    );
                    worker
                        .sender
                        .send(WorkerServerMessage::AskFiles(missing_files))?;
                    for _ in 0..num_missing {
                        let message = worker.receiver.recv()?;
                        if let WorkerClientMessage::ProvideFile(uuid, key) = message {
                            let handle = file_store
                                .store(&key, ChannelFileIterator::new(&worker.receiver))?;
                            output_handlers.insert(uuid, handle);
                        } else {
                            panic!("Unexpected message from worker: {:?}", message);
                        }
                    }
                    if let Err(e) = scheduler.send(SchedulerInMessage::WorkerResult {
                        worker: worker.uuid,
                        result,
                        outputs: output_handlers,
                    }) {
                        error!("Failed to send message to scheduler: {:?}", e);
                        break;
                    }
                }
            }
        }
        // when the worker disconnects, tell the scheduler that the worker is no longer alive (thus
        // rescheduling the job if needed).
        if scheduler
            .send(SchedulerInMessage::WorkerDisconnected { uuid: worker.uuid })
            .is_err()
        {
            debug!("Cannot tell the scheduler that a worker left, maybe it's gone");
        }
        // send back to the WorkerManager a message, letting it know that the worker is no longer
        // connected, thus removing it from the list.
        if worker_manager
            .send(WorkerManagerInMessage::WorkerDisconnected {
                worker: worker.uuid,
            })
            .is_err()
        {
            debug!("Worker manager is gone");
        }
        Ok(())
    }
}
