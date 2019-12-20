use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use task_maker_exec::executors::RemoteEntityMessage;
use task_maker_exec::{connect_channel, Worker};
use task_maker_store::FileStore;

use crate::error::NiceError;
use crate::opt::{Opt, WorkerOptions};
use crate::sandbox::self_exec_sandbox;

/// Entry point for the worker.
pub fn main_worker(opt: Opt, worker_opt: WorkerOptions) {
    let server_addr = SocketAddr::from_str(&worker_opt.server_addr)
        .nice_expect("Invalid server address provided");

    let store_path = opt.store_dir();
    let file_store = Arc::new(
        FileStore::new(
            store_path.join("store"),
            opt.max_cache * 1024 * 1024,
            opt.min_cache * 1024 * 1024,
        )
        .nice_expect("Cannot create the file store"),
    );
    let sandbox_path = store_path.join("sandboxes");
    let num_workers = opt.num_cores.unwrap_or_else(num_cpus::get);

    let mut workers = vec![];
    let name = opt
        .name
        .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
    for i in 0..num_workers {
        let (executor_tx, executor_rx) =
            connect_channel(server_addr).nice_expect("Failed to connect to the server");
        executor_tx
            .send(RemoteEntityMessage::Welcome { name: name.clone() })
            .nice_expect("Cannot send welcome to the server");
        let worker = Worker::new_with_channel(
            &format!("{} {}", name, i),
            file_store.clone(),
            sandbox_path.clone(),
            executor_tx.change_type(),
            executor_rx.change_type(),
            self_exec_sandbox,
        );
        workers.push(
            thread::Builder::new()
                .name(format!("Worker {}", worker))
                .spawn(move || {
                    worker.work().nice_expect("Worker failed");
                })
                .nice_expect("Failed to spawn worker"),
        );
    }
    for worker in workers.into_iter() {
        worker.join().nice_expect("Worker failed");
    }
}
