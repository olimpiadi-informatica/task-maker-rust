use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use task_maker_exec::{connect_channel, Worker};
use task_maker_store::FileStore;

use crate::opt::Opt;

/// Entry point for the worker.
pub fn main_worker(opt: Opt) {
    let server_addr = SocketAddr::from_str(
        &opt.worker_address_server
            .expect("Please provide the address to which connect to (--worker-address-server)"),
    )
    .expect("Invalid server address provided");

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
    let sandbox_path = store_path.join("sandboxes");
    let num_workers = opt.num_cores.unwrap_or_else(num_cpus::get);

    let mut workers = vec![];
    let name = opt
        .name
        .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
    for i in 0..num_workers {
        let (executor_tx, executor_rx) =
            connect_channel(server_addr).expect("Failed to connect to the server");
        let worker = Worker::new_with_channel(
            &format!("{} {}", name, i),
            file_store.clone(),
            sandbox_path.clone(),
            executor_tx,
            executor_rx,
        );
        workers.push(
            thread::Builder::new()
                .name(format!("Worker {}", worker))
                .spawn(move || {
                    worker.work().expect("Worker failed");
                })
                .expect("Failed to spawn worker"),
        );
    }
    for worker in workers.into_iter() {
        worker.join().expect("Worker failed");
    }
}
