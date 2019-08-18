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
//! [`tmbox`](https://github.com/veluca93/tmbox).

#![deny(missing_docs)]

extern crate failure;
#[macro_use]
extern crate log;
extern crate serde;
extern crate serde_json;
extern crate task_maker_dag;
extern crate task_maker_store;
extern crate tempdir;
extern crate uuid;
extern crate which;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use task_maker_dag::ExecutionDAG;
use task_maker_store::FileStore;

pub use client::*;
pub use executor::*;
use failure::Error;
pub use sandbox::*;
pub(crate) use scheduler::*;
pub(crate) use signals::*;
pub(crate) use worker::*;

mod client;
mod executor;
pub mod executors;
pub mod proto;
mod sandbox;
mod scheduler;
mod signals;
mod worker;

/// The channel part that sends data.
pub type ChannelSender = Sender<String>;
/// The channel part that receives data.
pub type ChannelReceiver = Receiver<String>;

/// Serialize a message into the sender serializing it.
pub fn serialize_into<T>(what: &T, sender: &ChannelSender) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender
        .send(serde_json::to_string(what)?)
        .map_err(|e| e.into())
}

/// Deserialize a message from the channel and return it.
pub fn deserialize_from<T>(reader: &ChannelReceiver) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

/// Evaluate a DAG locally spawning a new [`LocalExecutor`](executors/struct.LocalExecutor.html)
/// with the specified number of workers.
pub fn eval_dag_locally<P: Into<PathBuf>>(dag: ExecutionDAG, store_dir: P, num_cores: usize) {
    let (tx, rx_remote) = channel();
    let (tx_remote, rx) = channel();
    let store_dir = store_dir.into();
    let server = thread::spawn(move || {
        let file_store = FileStore::new(store_dir).expect("Cannot create the file store");
        let mut executor =
            executors::LocalExecutor::new(Arc::new(Mutex::new(file_store)), num_cores);
        executor.evaluate(tx_remote, rx_remote).unwrap();
    });
    ExecutorClient::evaluate(dag, tx, rx).unwrap();
    server.join().expect("Server panicked");
}

#[cfg(test)]
mod tests {
    extern crate pretty_assertions;

    use pretty_assertions::assert_eq;
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use task_maker_dag::*;
    use tempdir::TempDir;

    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        #[derive(Serialize, Deserialize)]
        struct Thing {
            pub x: u32,
            pub y: String,
        }

        let (tx, rx): (ChannelSender, ChannelReceiver) = channel();
        serialize_into(
            &Thing {
                x: 42,
                y: "foobar".into(),
            },
            &tx,
        )
        .unwrap();
        let thing: Thing = deserialize_from(&rx).unwrap();
        assert_eq!(thing.x, 42);
        assert_eq!(thing.y, "foobar");
    }

    #[test]
    fn test_local_evaluation() {
        let cwd = TempDir::new("tm-test").unwrap();
        let mut dag = ExecutionDAG::new();

        let file = File::new("Input file");

        let mut exec = Execution::new(
            "An execution",
            ExecutionCommand::System(PathBuf::from("true")),
        );
        exec.stdin(&file);
        let stdout = exec.stdout();

        let mut exec2 = Execution::new("Nope!", ExecutionCommand::System(PathBuf::from("false")));
        exec2.stdin(&stdout);
        let stdout2 = exec2.stdout();

        let mut exec3 = Execution::new("Skippp", ExecutionCommand::System(PathBuf::from("true")));
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
            exec_done.store(true, Ordering::Relaxed)
        });
        dag.on_execution_skip(&exec.uuid, || assert!(false, "exec has been skipped"));
        dag.on_execution_start(&exec.uuid, move |_w| {
            exec_start.store(true, Ordering::Relaxed)
        });
        dag.add_execution(exec);
        dag.on_execution_done(&exec2.uuid, move |_res| {
            exec2_done.store(true, Ordering::Relaxed)
        });
        dag.on_execution_skip(&exec2.uuid, || assert!(false, "exec2 has been skipped"));
        dag.on_execution_start(&exec2.uuid, move |_w| {
            exec2_start.store(true, Ordering::Relaxed)
        });
        dag.add_execution(exec2);
        dag.on_execution_done(&exec3.uuid, |_res| {
            assert!(false, "exec3 has not been skipped")
        });
        dag.on_execution_skip(&exec3.uuid, move || {
            exec3_skipped.store(true, Ordering::Relaxed)
        });
        dag.on_execution_start(&exec3.uuid, |_w| {
            assert!(false, "exec3 has not been skipped")
        });
        dag.add_execution(exec3);
        dag.write_file_to(&stdout, &cwd.path().join("stdout"));
        dag.write_file_to(&stdout2, &cwd.path().join("stdout2"));
        dag.write_file_to(&output3, &cwd.path().join("output3"));

        eval_dag_locally(dag, cwd.path(), 2);

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
