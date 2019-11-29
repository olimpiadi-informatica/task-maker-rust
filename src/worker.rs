use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use task_maker_exec::{connect_channel, serialize_into, Worker};
use task_maker_store::FileStore;

use crate::opt::{Opt, WorkerOptions};
use task_maker_exec::executors::RemoteEntityMessage;

/// Entry point for the worker.
pub fn main_worker(opt: Opt, worker_opt: WorkerOptions) {
    let server_addr =
        SocketAddr::from_str(&worker_opt.server_addr).expect("Invalid server address provided");

    let store_path = opt.store_dir();
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
        serialize_into(
            &RemoteEntityMessage::Welcome { name: name.clone() },
            &executor_tx,
        )
        .expect("Cannot send welcome to the server");
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
