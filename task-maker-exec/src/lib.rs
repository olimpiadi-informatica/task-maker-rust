//! DAG execution dispatching the tasks to the workers.
//!
//! This crate is able to setup a distributed execution environment by creating an `Executor` which
//! spawns some workers, and a client which, talking to the executor, is able to schedule the
//! execution of a DAG.
//!
//! A [`FileStore`](../task_maker_store/struct.FileStore.html) is used to store the files of the DAG
//! and [`std::sync::mpsc::channel`](https://doc.rust-lang.org/std/sync/mpsc/fn.channel.html) is
//! used for the internal communication.
//!
//! A simple `Scheduler` is used to dispatch the jobs when all their dependencies are ready. When an
//! execution is not successful (i.e. does not return with zero) all the depending jobs are
//! cancelled.
//!
//! All the tasks are run inside a [`Sandbox`](struct.Sandbox.html) provided by
//! [`tabox`](https://crates.io/crates/tabox).
//!
//! ## Implementation details
//!
//! In order to support both local and remote evaluation, sharing the same code and logic we need a
//! layer of abstraction over the execution.
//!
//! The components of the execution are:
//!
//! - [`ExecutorClient`](struct.ExecutorClient.html) (from now `Client`) is the component that runs
//!   on the process that wants to execute the DAG, it asks to its executor to run it and gets the
//!   results eventually processing them.
//! - [`LocalExecutor`](executors/struct.LocalExecutor.html) the component to which the `Client`
//!   connects to for executing a DAG locally, it internally uses an `Executor` for actually running
//!   the DAG, but also spawns local workers.
//! - [`RemoteExecutor`](executors/struct.RemoteExecutor.html) the component to which the `Client`
//!   connects to for executing a DAG remotely. This usually runs on a remote machine w.r.t the
//!   client's one. It internally uses an `Executor` for running the DAG but doesn't spawn the
//!   workers since they are remote too. This component is also responsible for listening to the
//!   sockets for the connections of the clients and the workers.
//! - `Executor` the component that abstracts the connection of workers and clients and handles the
//!   actual communication between the scheduler and the clients. This component is also responsible
//!   for spawning the scheduler and keeping it working.
//! - `Scheduler` the component that, given DAGs and notification about the status of the workers
//!   (i.e. worker connection/disconnection/job completion) schedules the execution of the ready
//!   jobs and sends to the clients the notification about their status.
//! - `WorkerManager` the component that handles the connections with the workers and notifies the
//!   scheduler about worker events.
//! - `Worker` the component that actively asks for work to do, waiting a response from the
//!   scheduler, and eventually receive and execute it. After the execution completes, the worker
//!   asks for another job to do.
//!
//! ### Local execution
//!
//! ![Local execution diagram](https://www.lucidchart.com/publicSegments/view/f0ad0719-2dd4-4c60-9da0-ca0614fd37a1/image.png)
//!
//! Running locally all the components are spawn in the current process, but different threads.
//! The communication between them is done using in memory channels, including the one from the
//! `Client` to the `LocalExecutor`.
//!
//! The `Client` connects to the `LocalExecutor`, specifying also the number of workers to spawn.
//! The local executor spawns an actual executor, a worker manager and that number of workers.
//!
//! ### Remote execution
//!
//! ![Local execution diagram](https://www.lucidchart.com/publicSegments/view/86167d8f-b231-4698-a8ac-a26ed995fd19/image.png)
//!
//! In a remote environment the `Client` connects to the `RemoteExecutor` via a TCP socket, the
//! remote executor listens for client and worker connections. The workers then connect to the
//! `RemoteExecutor` and they are handled by the `WorkerManager`.

#![deny(missing_docs)]
#![allow(clippy::upper_case_acronyms)]

#[macro_use]
extern crate log;
#[macro_use(defer)]
extern crate scopeguard;

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use anyhow::Error;
/// Re-export `ductile` since it's sensible to any version change
pub use ductile;
use ductile::new_local_channel;
use scrypt::ScryptParams;

pub use client::ExecutorClient;
pub use executor::{ExecutorStatus, ExecutorWorkerStatus, WorkerCurrentJobStatus};
pub use sandbox::RawSandboxResult;
pub use sandbox_runner::{ErrorSandboxRunner, SandboxRunner, SuccessSandboxRunner};
pub use scheduler::ClientInfo;
use task_maker_cache::Cache;
use task_maker_dag::ExecutionDAG;
use task_maker_store::FileStore;
pub use worker::{Worker, WorkerConn};

mod check_dag;
mod client;
pub mod detect_exe;
mod executor;
pub mod executors;
pub mod find_tools;
pub mod proto;
pub mod sandbox;
mod sandbox_runner;
mod scheduler;
mod worker;
mod worker_manager;

/// Derive the encryption key from a password string.
pub fn derive_key_from_password<S: AsRef<str>>(password: S) -> [u8; 32] {
    let mut key = [0u8; 32];
    scrypt::scrypt(
        password.as_ref().as_bytes(),
        b"task-maker",
        &ScryptParams::new(8, 8, 1).unwrap(),
        &mut key,
    )
    .expect("Failed to derive key from password");
    key
}

/// Evaluate a DAG locally spawning a new [`LocalExecutor`](executors/struct.LocalExecutor.html)
/// with the specified number of workers.
pub fn eval_dag_locally<P: Into<PathBuf>, P2: Into<PathBuf>, R>(
    dag: ExecutionDAG,
    store_dir: P,
    num_cores: usize,
    sandbox_path: P2,
    max_cache: u64,
    min_cache: u64,
    sandbox_runner: R,
) where
    R: SandboxRunner + 'static,
{
    // FIXME: this function may return Result<(), Error>
    let (tx, rx_remote) = new_local_channel();
    let (tx_remote, rx) = new_local_channel();
    let store_dir = store_dir.into();
    let sandbox_path = sandbox_path.into();
    let file_store = Arc::new(
        FileStore::new(&store_dir, max_cache, min_cache).expect("Cannot create the file store"),
    );
    let server_file_store = file_store.clone();
    let server = thread::Builder::new()
        .name("Local executor".into())
        .spawn(move || {
            let cache = Cache::new(store_dir).expect("Cannot create the cache");
            let executor =
                executors::LocalExecutor::new(server_file_store, num_cores, sandbox_path);
            executor
                .evaluate(tx_remote, rx_remote, cache, sandbox_runner)
                .expect("Executor failed");
        })
        .expect("Failed to spawn local executor thread");
    ExecutorClient::evaluate(dag, tx, &rx, file_store, |_| Ok(())).expect("Client failed");
    server.join().expect("Server panicked");
}

#[cfg(test)]
mod tests {
    extern crate pretty_assertions;

    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tempdir::TempDir;

    use task_maker_dag::*;

    use crate::sandbox_runner::UnsafeSandboxRunner;

    use super::*;

    #[test]
    fn test_local_evaluation() {
        let cwd = TempDir::new("tm-test").unwrap();
        let mut dag = ExecutionDAG::new();

        let file = File::new("Input file");

        let mut exec = Execution::new("An execution", ExecutionCommand::system("true"));
        exec.stdin(&file);
        let stdout = exec.stdout();

        let mut exec2 = Execution::new("Nope!", ExecutionCommand::system("false"));
        exec2.stdin(&stdout);
        let stdout2 = exec2.stdout();

        let mut exec3 = Execution::new("Skippp", ExecutionCommand::system("true"));
        exec3.stdin(&stdout2);
        let output3 = exec3.output(Path::new("test"));

        let exec_done = Arc::new(AtomicBool::new(false));
        let exec_done2 = exec_done.clone();
        let exec_start = Arc::new(AtomicBool::new(false));
        let exec_start2 = exec_start.clone();
        let exec2_done = Arc::new(AtomicBool::new(false));
        let exec2_done2 = exec2_done.clone();
        let exec2_start = Arc::new(AtomicBool::new(false));
        let exec2_start2 = exec2_start.clone();
        let exec3_skipped = Arc::new(AtomicBool::new(false));
        let exec3_skipped2 = exec3_skipped.clone();
        dag.provide_file(file, Path::new("/dev/null")).unwrap();
        dag.on_execution_done(&exec.uuid, move |_res| {
            exec_done.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.on_execution_skip(&exec.uuid, || panic!("exec has been skipped"));
        dag.on_execution_start(&exec.uuid, move |_w| {
            exec_start.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.add_execution(exec);
        dag.on_execution_done(&exec2.uuid, move |_res| {
            exec2_done.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.on_execution_skip(&exec2.uuid, || panic!("exec2 has been skipped"));
        dag.on_execution_start(&exec2.uuid, move |_w| {
            exec2_start.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.add_execution(exec2);
        dag.on_execution_done(&exec3.uuid, |_res| panic!("exec3 has not been skipped"));
        dag.on_execution_skip(&exec3.uuid, move || {
            exec3_skipped.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.on_execution_start(&exec3.uuid, |_w| panic!("exec3 has not been skipped"));
        dag.add_execution(exec3);
        dag.write_file_to(&stdout, &cwd.path().join("stdout"), false);
        dag.write_file_to(&stdout2, &cwd.path().join("stdout2"), false);
        dag.write_file_to(&output3, &cwd.path().join("output3"), false);

        eval_dag_locally(
            dag,
            cwd.path(),
            2,
            cwd.path(),
            1000,
            1000,
            UnsafeSandboxRunner::default(),
        );

        assert!(exec_done2.load(Ordering::Relaxed));
        assert!(exec_start2.load(Ordering::Relaxed));
        assert!(exec2_done2.load(Ordering::Relaxed));
        assert!(exec2_start2.load(Ordering::Relaxed));
        assert!(exec3_skipped2.load(Ordering::Relaxed));
        assert!(cwd.path().join("stdout").exists());
        assert!(!cwd.path().join("stdout2").exists());
        assert!(!cwd.path().join("output3").exists());
    }
}
