use crate::*;
use failure::{format_err, Error};
use std::sync::Arc;
use std::thread;
use task_maker_cache::Cache;

/// An Executor that runs locally by spawning a number of threads with the workers inside.
pub struct LocalExecutor {
    executor: Executor,
    /// A reference to the [`FileStore`](../../task_maker_store/struct.FileStore.html).
    file_store: Arc<FileStore>,
    /// Where to store the sandboxes of the workers.
    sandbox_path: PathBuf,
    /// The number of local workers to spawn.
    pub num_workers: usize,
}

impl LocalExecutor {
    /// Make a new [`LocalExecutor`](struct.LocalExecutor.html) based on a
    /// [`FileStore`](../../task_maker_store/struct.FileStore.html) and ready to spawn that number
    /// of workers.
    pub fn new<P: Into<PathBuf>>(
        file_store: Arc<FileStore>,
        num_workers: usize,
        sandbox_path: P,
    ) -> LocalExecutor {
        LocalExecutor {
            executor: Executor::new(file_store.clone()),
            num_workers,
            file_store,
            sandbox_path: sandbox_path.into(),
        }
    }

    /// Starts the Executor spawning the workers on new threads and blocking on the `Executor`
    /// thread.
    ///
    /// * `sender` - Channel that sends messages to the client.
    /// * `receiver` - Channel that receives messages from the client.
    /// * `cache` - The cache the executor has to use.
    pub fn evaluate(
        self,
        sender: ChannelSender,
        receiver: ChannelReceiver,
        cache: Cache,
    ) -> Result<(), Error> {
        info!("Spawning {} workers", self.num_workers);

        let mut worker_manager =
            WorkerManager::new(self.file_store.clone(), self.executor.scheduler_tx.clone());

        let mut workers = vec![];
        for i in 0..self.num_workers {
            let (worker, conn) = Worker::new(
                &format!("Local worker {}", i),
                self.file_store.clone(),
                self.sandbox_path.clone(),
            );
            workers.push(worker_manager.add(conn));
            workers.push(
                thread::Builder::new()
                    .name(format!("Worker {}", worker))
                    .spawn(move || {
                        worker.work().expect("Worker failed");
                    })?,
            );
        }
        self.executor.evaluate(sender, receiver, cache)?;
        worker_manager.stop()?;
        for worker in workers.into_iter() {
            worker
                .join()
                .map_err(|e| format_err!("Worker panicked: {:?}", e))?;
        }
        Ok(())
    }
}
