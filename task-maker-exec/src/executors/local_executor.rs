use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

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
    /// The number of local workers to spawn.
    pub num_workers: usize,
    /// The internal executor.
    executor: Executor,
    /// Channel sending messages to the executor.
    executor_tx: Sender<ExecutorInMessage>,
    /// Join handle of the spawned workers.
    workers: Vec<JoinHandle<Result<(), Error>>>,
}

impl LocalExecutor {
    /// Make a new [`LocalExecutor`] based on a [`FileStore`] and ready to spawn that number of
    /// workers using a [`Cache`].
    pub fn new<P: Into<PathBuf>, R>(
        file_store: Arc<FileStore>,
        cache: Cache,
        num_workers: usize,
        sandbox_path: P,
        sandbox_runner: R,
    ) -> Result<LocalExecutor, Error>
    where
        R: SandboxRunner + 'static,
    {
        let sandbox_path = sandbox_path.into();
        let (executor_tx, executor_rx) = channel();
        let executor = Executor::new(file_store.clone(), cache, executor_rx, false);

        // share the runner for all the workers
        let sandbox_runner = Arc::new(sandbox_runner);

        info!("Spawning {} workers", num_workers);
        let mut workers = vec![];
        // spawn the workers and connect them to the executor
        for i in 0..num_workers {
            let runner = sandbox_runner.clone();
            let (worker, conn) = Worker::new(
                format!("Local worker {}", i),
                file_store.clone(),
                #[allow(clippy::needless_borrow)]
                &sandbox_path,
                runner,
            )
            .context("Failed to start local worker")?;
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

        Ok(LocalExecutor {
            num_workers,
            executor,
            executor_tx,
            workers,
        })
    }

    /// Starts the Executor spawning the workers on new threads and blocking on the `Executor`
    /// thread.
    ///
    /// * `sender` - Channel that sends messages to the client.
    /// * `receiver` - Channel that receives messages from the client.
    pub fn evaluate(
        self,
        sender: ChannelSender<ExecutorServerMessage>,
        receiver: ChannelReceiver<ExecutorClientMessage>,
    ) -> Result<(), Error> {
        // tell the executor that it has a new (local) client. Since the executor is not in
        // long_running mode, after this client is done the executor will exit.
        self.executor_tx
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
        drop(self.executor_tx);
        // this method will block until all the operations are done
        self.executor
            .run()
            .context("Local executor failed to evaluate")?;

        // since the executor is going down the worker are disconnecting
        for worker in self.workers.into_iter() {
            worker
                .join()
                .map_err(|e| anyhow!("Worker panicked: {:?}", e))?
                .context("Worker failed")?;
        }
        Ok(())
    }
}
