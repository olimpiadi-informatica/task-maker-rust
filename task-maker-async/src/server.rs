use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::dag::{ExecutionDAG, ExecutionDAGOptions, ExecutionFileMode, ExecutionGroup, Niceness};
use crate::error::Error;
use crate::store::{FileSetHash, FileSetWriteHandle, Store, StoreService, WaitFor};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use tarpc::context::{self, Context};
use tokio::sync::oneshot::{channel, Sender};

const MAX_RETRIES: usize = 4;

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    // TODO: decide what we want to report here.
}

#[tarpc::service]
pub trait Server {
    /// Asks the server to evaluate the given DAG. All the input files must already be available in
    /// the Store.
    async fn evaluate(dag: ExecutionDAG, options: ExecutionDAGOptions) -> Result<(), Error>;

    /// Asks the server for work to do. Returns a FileSetHandle to be used to store the
    /// outputs in the Store. id is an identifier of the worker that calls the method.
    async fn get_work(id: usize) -> (ExecutionGroup, ExecutionDAGOptions, FileSetWriteHandle);

    /// Retrieves information about the status of the server.
    async fn get_status() -> ServerStatus;
}

#[derive(Eq, PartialEq)]
struct WorkerTask {
    scheduling: (Niceness, Niceness, Instant),
    execution: ExecutionGroup,
    options: ExecutionDAGOptions,
    handle: FileSetWriteHandle,
}

impl Ord for WorkerTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Inverted order, so that tasks with lowest niceness are preferred, and on equality tasks
        // that became ready first are preferred.
        other.scheduling.cmp(&self.scheduling)
    }
}

impl PartialOrd for WorkerTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ServerImpl {
    waiting_workers: Vec<Sender<()>>,
    queue: BinaryHeap<WorkerTask>,
    store: StoreService,
}

#[derive(Clone)]
pub struct ServerService {
    service: Arc<Mutex<ServerImpl>>,
    // TODO(veluca): probably needs a CancellationToken too.
}

async fn wait_for_dependency(store: StoreService, hash: FileSetHash) -> Result<(), Error> {
    store
        .wait_for_fileset(
            context::current(),
            hash,
            crate::store::WaitFor::Finalization,
        )
        .await
}

impl ServerService {
    async fn evaluate_execution_group(
        self,
        execution: ExecutionGroup,
        options: ExecutionDAGOptions,
    ) -> Result<(), Error> {
        let hash = FileSetHash {
            data: execution.get_data_identification_hash(),
            variant: execution.get_variant_identification_hash(),
        };
        let dependencies: Vec<_> = execution
            .executions
            .iter()
            .flat_map(|ex| ex.files.iter())
            .flat_map(|(_, f)| {
                match f {
                    ExecutionFileMode::Input(info) => Some(info.hash),
                    _ => None,
                }
                .into_iter()
            })
            .collect();

        // TODO(veluca): check for compatible variants.

        let store = {
            let service = self.service.lock().unwrap();
            service.store.clone()
        };

        // TODO(veluca): implement skipping things that depend on failed tasks, and in general
        // declaring executions as being skipped.
        try_join_all(
            dependencies
                .iter()
                .map(|hash| wait_for_dependency(store.clone(), *hash)),
        )
        .await?;

        for _ in 0..MAX_RETRIES {
            let store = store.clone();
            let computation = store.create_computation(hash)?;
            if computation.is_none() {
                return Ok(());
            }
            let computation = computation.unwrap();

            // TODO(veluca): modify the code so that the fileset is only created (and waited for)
            // once a worker picks up the task.
            let task = WorkerTask {
                scheduling: (options.niceness, execution.niceness, Instant::now()),
                execution: execution.clone(),
                options,
                handle: computation,
            };

            {
                let mut service = self.service.lock().unwrap();
                service.queue.push(task);
                // Wake up all the workers that are currently waiting for a new task.
                // TODO(veluca): consider only waking up one worker that is still waiting.
                service.waiting_workers.drain(..).for_each(|sender| {
                    sender.send(()).unwrap();
                });
            }

            // Wait for the task to be executed by the worker.
            let res = store
                .wait_for_fileset(context::current(), hash, WaitFor::Finalization)
                .await;
            if let Err(err) = res {
                if matches!(err, Error::FileSetDropped(_)) {
                    // The worker failed.
                    continue;
                }
                return Err(err);
            }
            // If waiting was successful, everything is done.
            return Ok(());
        }

        // TODO(veluca): proper logging.
        eprintln!("Execution failed too many times: {:?}", execution);

        Err(Error::ExecutionFailure(hash))
    }
}

#[tarpc::server]
impl Server for ServerService {
    async fn evaluate(
        self,
        _context: Context,
        dag: ExecutionDAG,
        options: ExecutionDAGOptions,
    ) -> Result<(), Error> {
        // TODO(veluca): handle evaluation requests being dropped.
        // TODO(veluca): check the input is actually a DAG. If not, this code will deadlock.
        try_join_all(
            dag.execution_groups
                .into_iter()
                .map(|group| self.clone().evaluate_execution_group(group, options)),
        )
        .await?;
        Ok(())
    }

    async fn get_work(
        self,
        _context: Context,
        _id: usize,
    ) -> (ExecutionGroup, ExecutionDAGOptions, FileSetWriteHandle) {
        loop {
            let (receiver, task) = {
                let mut service = self.service.lock().unwrap();
                if service.queue.is_empty() {
                    let (sender, receiver) = channel();
                    service.waiting_workers.push(sender);
                    (Some(receiver), None)
                } else {
                    (None, service.queue.pop())
                }
            };
            if let Some(task) = task {
                return (task.execution, task.options, task.handle);
            }
            receiver.unwrap().await.unwrap();
        }
    }

    async fn get_status(self, _context: Context) -> ServerStatus {
        ServerStatus {}
    }
}
