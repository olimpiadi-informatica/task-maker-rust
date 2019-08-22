use crate::*;
use failure::{format_err, Error};
use std::sync::{Arc, Mutex};
use std::thread;
use task_maker_cache::Cache;

/// An Executor that runs locally by spawning a number of threads with the workers inside.
pub struct LocalExecutor {
    /// The real Executor that does the actual work.
    executor: Executor,
    /// A reference to the [`FileStore`](../../task_maker_store/struct.FileStore.html).
    file_store: Arc<Mutex<FileStore>>,
    /// The number of local workers to spawn.
    pub num_workers: usize,
}

impl LocalExecutor {
    /// Make a new [`LocalExecutor`](struct.LocalExecutor.html) based on a
    /// [`FileStore`](../../task_maker_store/struct.FileStore.html) and ready to spawn that number
    /// of workers.
    pub fn new(
        file_store: Arc<Mutex<FileStore>>,
        cache: Cache,
        num_workers: usize,
    ) -> LocalExecutor {
        LocalExecutor {
            executor: Executor::new(file_store.clone(), cache),
            file_store: file_store.clone(),
            num_workers,
        }
    }

    /// Starts the Executor spawning the workers on new threads and blocking on the `Executor`
    /// thread.
    ///
    /// * `sender` - Channel that sends messages to the client.
    /// * `receiver` - Channel that receives messages from the client.
    pub fn evaluate(
        &mut self,
        sender: ChannelSender,
        receiver: ChannelReceiver,
    ) -> Result<(), Error> {
        info!("Spawning {} workers", self.num_workers);
        let mut workers = vec![];
        for i in 0..self.num_workers {
            let (worker, conn) =
                Worker::new(&format!("Local worker {}", i), self.file_store.clone());
            workers.push(self.executor.add_worker(conn));
            workers.push(
                thread::Builder::new()
                    .name(format!("Worker {}", worker))
                    .spawn(move || {
                        worker.work().expect("Worker failed");
                    })?,
            );
        }
        self.executor.evaluate(sender, receiver)?;
        for worker in workers.into_iter() {
            worker
                .join()
                .map_err(|e| format_err!("Worker panicked: {:?}", e))?;
        }
        Ok(())
    }
}
