use std::sync::Arc;

use anyhow::{bail, Context, Error};

use task_maker_exec::executors::{RemoteEntityMessage, RemoteEntityMessageResponse};
use task_maker_exec::Worker;
use task_maker_store::FileStore;

use crate::remote::connect_to_remote_server;
use crate::sandbox::SelfExecSandboxRunner;
use crate::tools::opt::WorkerOpt;

/// Version of task-maker
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Entry point for the worker.
pub fn main_worker(opt: WorkerOpt) -> Result<(), Error> {
    let store_path = opt.storage.store_dir();
    let file_store = Arc::new(
        FileStore::new(
            store_path.join("store"),
            opt.storage.max_cache * 1024 * 1024,
            opt.storage.min_cache * 1024 * 1024,
        )
        .context("Cannot create the file store")?,
    );
    let sandbox_path = store_path.join("sandboxes");

    let name = opt
        .name
        .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
    let (executor_tx, executor_rx) = connect_to_remote_server(&opt.server_addr, 27183)
        .context("Failed to connect to the server")?;
    executor_tx
        .send(RemoteEntityMessage::Welcome {
            name: name.clone(),
            version: VERSION.into(),
        })
        .context("Cannot send welcome to the server")?;
    if let RemoteEntityMessageResponse::Rejected(err) = executor_rx
        .recv()
        .context("Remote executor didn't reply to the welcome message")?
    {
        bail!("The server rejected the worker connection: {}", err);
    }

    let name = if let Some(wid) = opt.worker_id {
        format!("{} {}", name, wid)
    } else {
        name
    };

    let worker = Worker::new_with_channel(
        &name,
        file_store,
        sandbox_path,
        executor_tx.change_type(),
        executor_rx.change_type(),
        SelfExecSandboxRunner::default(),
    );
    worker.work()
}
