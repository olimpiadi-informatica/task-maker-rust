use crate::opt::{Opt, ServerOptions};
use std::sync::Arc;
use task_maker_cache::Cache;
use task_maker_exec::executors::RemoteExecutor;
use task_maker_store::FileStore;

/// Entry point for the server.
pub fn main_server(opt: Opt, server_opt: ServerOptions) {
    // setup the executor
    let store_path = opt.store_dir();
    let file_store =
        Arc::new(FileStore::new(store_path.join("store")).expect("Cannot create the file store"));
    let cache = Cache::new(store_path.join("cache")).expect("Cannot create the cache");

    let remote_executor = RemoteExecutor::new(file_store);

    remote_executor.start(&server_opt.client_addr, &server_opt.worker_addr, cache);
}
