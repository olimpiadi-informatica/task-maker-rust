use std::sync::Arc;

use anyhow::{Context, Error};

use task_maker_cache::Cache;
use task_maker_exec::executors::RemoteExecutor;
use task_maker_store::FileStore;

use crate::tools::opt::ServerOpt;

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
