use crate::opt::Opt;
use std::sync::Arc;
use task_maker_cache::Cache;
use task_maker_exec::executors::RemoteExecutor;
use task_maker_store::FileStore;

/// Entry point for the server.
pub fn main_server(opt: Opt) {
    // setup the executor
    let (store_path, _tempdir) = match opt.store_dir {
        Some(dir) => (dir, None),
        None => {
            let cwd =
                tempdir::TempDir::new("task-maker").expect("Failed to create temporary directory");
            (cwd.path().to_owned(), Some(cwd))
        }
    };
    let file_store =
        Arc::new(FileStore::new(store_path.join("store")).expect("Cannot create the file store"));
    let cache = Cache::new(store_path.join("cache")).expect("Cannot create the cache");

    let remote_executor = RemoteExecutor::new(file_store);
    remote_executor.start(
        &opt.server_address_clients,
        &opt.server_address_workers,
        cache,
    );
}
