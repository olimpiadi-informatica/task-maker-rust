use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Context, Error};
use ductile::{ChannelReceiver, ChannelSender};
use uuid::Uuid;

use task_maker_cache::Cache;
use task_maker_store::FileStore;

use crate::executor::{Executor, ExecutorInMessage};
use crate::proto::{ExecutorClientMessage, ExecutorServerMessage};
use crate::sandbox_runner::SandboxRunner;
use crate::scheduler::ClientInfo;
use crate::Worker;

/// An Executor that runs locally by spawning a number of threads with the workers inside.
pub struct LocalExecutor {
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
    /// * `sandbox_runner` - The function to call for running a process in a sandbox.
    pub fn evaluate<R>(
        self,
        sender: ChannelSender<ExecutorServerMessage>,
        receiver: ChannelReceiver<ExecutorClientMessage>,
        cache: Cache,
        sandbox_runner: R,
    ) -> Result<(), Error>
    where
        R: SandboxRunner + 'static,
    {
        let (executor_tx, executor_rx) = channel();
        let executor = Executor::new(self.file_store.clone(), cache, executor_rx, false);

        // share the runner for all the workers
        let sandbox_runner = Arc::new(sandbox_runner);

        info!("Spawning {} workers", self.num_workers);
        let mut workers = vec![];
        // spawn the workers and connect them to the executor
        for i in 0..self.num_workers {
            let runner = sandbox_runner.clone();
            let (worker, conn) = Worker::new(
                format!("Local worker {}", i),
                self.file_store.clone(),
                self.sandbox_path.clone(),
                runner,
            );
            executor_tx
                .send(ExecutorInMessage::WorkerConnected { worker: conn })
                .map_err(|e| anyhow!("Failed to send WorkerConnected: {:?}", e))?;
            let worker_name = format!("Worker {}", worker);
            workers.push(
                thread::Builder::new()
                    .name(worker_name.clone())
                    .spawn(move || worker.work())
                    .with_context(|| {
                        format!("Failed to start worker thread named '{}'", &worker_name)
                    })?,
            );
        }

        // tell the executor that it has a new (local) client. Since the executor is not in
        // long_running mode, after this client is done the executor will exit.
        executor_tx
            .send(ExecutorInMessage::ClientConnected {
                client: ClientInfo {
                    uuid: Uuid::new_v4(),
                    name: "Local client".to_string(),
                },
                sender,
                receiver,
            })
            .map_err(|e| anyhow!("Failed to send ClientConnected: {:?}", e))?;

        // no new client/worker can connect, make the executor stop accepting connections
        drop(executor_tx);
        // this method will block until all the operations are done
        executor
            .run()
            .context("Local executor failed to evaluate")?;

        // since the executor is going down the worker are disconnecting
        for worker in workers.into_iter() {
            worker
                .join()
                .map_err(|e| anyhow!("Worker panicked: {:?}", e))?
                .context("Worker failed")?;
        }
        Ok(())
    }
}
