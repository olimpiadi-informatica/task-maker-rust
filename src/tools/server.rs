use std::sync::Arc;

use anyhow::{Context, Error};
use clap::Parser;
use task_maker_cache::Cache;
use task_maker_exec::executors::RemoteExecutor;
use task_maker_store::FileStore;

use crate::StorageOpt;

#[derive(Parser, Debug, Clone)]
pub struct ServerOpt {
    /// Address to bind the server on for listening for the clients
    #[clap(default_value = "0.0.0.0:27182")]
    pub client_addr: String,

    /// Address to bind the server on for listening for the workers
    #[clap(default_value = "0.0.0.0:27183")]
    pub worker_addr: String,

    /// Password for the connection of the clients
    #[clap(long = "client-password")]
    pub client_password: Option<String>,

    /// Password for the connection of the workers
    #[clap(long = "worker-password")]
    pub worker_password: Option<String>,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: StorageOpt,
}

/// Entry point for the server.
pub fn main_server(opt: ServerOpt) -> Result<(), Error> {
    // setup the executor
    let store_path = opt.storage.store_dir();
    let file_store = Arc::new(
        FileStore::new(
            store_path.join("store"),
            opt.storage.max_cache * 1024 * 1024,
            opt.storage.min_cache * 1024 * 1024,
        )
        .context("Cannot create the file store")?,
    );
    let cache = Cache::new(store_path.join("cache")).context("Cannot create the cache")?;

    let remote_executor = RemoteExecutor::new(file_store);

    remote_executor.start(
        &opt.client_addr,
        &opt.worker_addr,
        opt.client_password,
        opt.worker_password,
        cache,
    )
}
