use crate::executor::*;
use crate::store::*;
use failure::Error;
use std::sync::{Arc, Mutex};
use std::thread;

/// An Executor that runs locally
pub struct LocalExecutor {
    /// The real Executor that does the work
    executor: Executor,
    /// A reference to the FileStore
    file_store: Arc<Mutex<FileStore>>,
    /// The number of local workers to spawn
    pub num_workers: usize,
}

impl LocalExecutor {
    /// Make a new LocalExecutor based on a FileStore and ready to spawn that
    /// number of workers
    pub fn new(file_store: Arc<Mutex<FileStore>>, num_workers: usize) -> LocalExecutor {
        LocalExecutor {
            executor: Executor::new(file_store.clone()),
            file_store: file_store.clone(),
            num_workers,
        }
    }

    /// Starts the Executor spawning the workers on new threads and blocking on
    /// the Executor thread.
    ///
    /// * `sender` - Channel that sends messages to the client
    /// * `receiver` - Channel that receives messages from the client
    pub fn evaluate(
        &mut self,
        sender: ChannelSender,
        receiver: ChannelReceiver,
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
