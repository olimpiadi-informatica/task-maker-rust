use crate::executor::*;
use crate::store::*;
use failure::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct LocalExecutor {
    executor: Executor,
    file_store: Arc<Mutex<FileStore>>,
    pub num_workers: usize,
}

impl LocalExecutor {
    pub fn new(file_store: Arc<Mutex<FileStore>>, num_workers: usize) -> LocalExecutor {
        LocalExecutor {
            executor: Executor::new(file_store.clone()),
            file_store: file_store.clone(),
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
        info!("Spawning {} workers", self.num_workers);
        for i in 0..self.num_workers {
            let (worker, conn) =
                Worker::new(&format!("Local worker {}", i), self.file_store.clone());
            self.executor.add_worker(conn);
            thread::Builder::new()
                .name(format!("Worker {}", worker))
                .spawn(move || {
                    worker.work().expect("Worker failed");
                })
                .expect("Failed to spawn worker thread");
        }
        self.executor.evaluate(sender, receiver)
    }
}
