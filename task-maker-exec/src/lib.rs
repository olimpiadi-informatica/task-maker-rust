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

#[macro_use]
extern crate log;
#[macro_use(defer)]
extern crate scopeguard;

use crossbeam_channel::{unbounded, Receiver, Sender};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use bincode;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use failure::Error;

pub use client::ExecutorClient;
pub use executor::ExecutorStatus;
use failure::_core::ops::Deref;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use task_maker_cache::Cache;
use task_maker_dag::ExecutionDAG;
use task_maker_store::FileStore;
pub use worker::{Worker, WorkerConn};

mod check_dag;
mod client;
mod executor;
pub mod executors;
pub mod proto;
mod sandbox;
mod scheduler;
mod worker;
mod worker_manager;
pub use sandbox::RawSandboxResult;
use tabox::configuration::SandboxConfiguration;

/// The channel part that sends data.
#[derive(Debug, Clone)]
pub enum ChannelSender {
    /// The connection is only a local in-memory channel.
    Local(Sender<Vec<u8>>),
    /// The connection is with a remote party.
    Remote(Arc<Mutex<TcpStream>>),
}

/// The channel part that receives data.
#[derive(Debug)]
pub enum ChannelReceiver {
    /// The connection is only a local in-memory channel.
    Local(Receiver<Vec<u8>>),
    /// The connection is with a remote party.
    Remote(RefCell<TcpStream>),
}

impl ChannelSender {
    /// Send some data into the channel.
    pub fn send(&self, data: Vec<u8>) -> Result<(), Error> {
        match self {
            ChannelSender::Local(sender) => sender.send(data).map_err(|e| e.into()),
            ChannelSender::Remote(sender) => {
                let mut sender = sender.lock().expect("Cannot lock ChannelSender");
                sender.write_u32::<LittleEndian>(data.len() as u32)?;
                sender.write_all(&data).map_err(|e| e.into())
            }
        }
    }
}

impl ChannelReceiver {
    /// Receive a message from the channel.
    pub fn recv(&self) -> Result<Vec<u8>, Error> {
        match self {
            ChannelReceiver::Local(receiver) => receiver.recv().map_err(|e| e.into()),
            ChannelReceiver::Remote(receiver) => {
                let mut receiver = receiver.borrow_mut();
                let len = receiver.read_u32::<LittleEndian>()?;
                let mut buf = vec![0; len as usize];
                receiver.read_exact(&mut buf)?;
                Ok(buf)
            }
        }
    }
}

/// Make a new pair of `ChannelSender` / `ChannelReceiver`
pub fn new_local_channel() -> (ChannelSender, ChannelReceiver) {
    let (tx, rx) = unbounded();
    (ChannelSender::Local(tx), ChannelReceiver::Local(rx))
}

/// Listener for connections on some TCP socket.
pub struct ChannelServer(TcpListener);

impl ChannelServer {
    /// Bind a socket and create a new `ChannelServer`.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<ChannelServer, Error> {
        Ok(ChannelServer(TcpListener::bind(addr)?))
    }
}

impl Deref for ChannelServer {
    type Target = TcpListener;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Iterator for ChannelServer {
    type Item = (ChannelSender, ChannelReceiver, SocketAddr);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self
                .0
                .incoming()
                .next()
                .expect("TcpListener::incoming returned None");
            if let Ok(s) = next {
                let peer_addr = s.peer_addr().expect("Peer has no remote address");
                let s2 = s.try_clone().expect("Failed to clone the stream");
                return Some((
                    ChannelSender::Remote(Arc::new(Mutex::new(s))),
                    ChannelReceiver::Remote(RefCell::new(s2)),
                    peer_addr,
                ));
            }
        }
    }
}

/// Connect to a remote channel.
pub fn connect_channel<A: ToSocketAddrs>(
    addr: A,
) -> Result<(ChannelSender, ChannelReceiver), Error> {
    let stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;
    Ok((
        ChannelSender::Remote(Arc::new(Mutex::new(stream))),
        ChannelReceiver::Remote(RefCell::new(stream2)),
    ))
}

/// Serialize a message into the sender serializing it.
pub fn serialize_into<T>(what: &T, sender: &ChannelSender) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender.send(bincode::serialize(what)?)
}

/// Deserialize a message from the channel and return it.
pub fn deserialize_from<T>(reader: &ChannelReceiver) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    bincode::deserialize(&data).map_err(|e| e.into())
}

/// Evaluate a DAG locally spawning a new [`LocalExecutor`](executors/struct.LocalExecutor.html)
/// with the specified number of workers.
pub fn eval_dag_locally<P: Into<PathBuf>, P2: Into<PathBuf>, F>(
    dag: ExecutionDAG,
    store_dir: P,
    num_cores: usize,
    sandbox_path: P2,
    max_cache: u64,
    min_cache: u64,
    sandbox_runner: F,
) where
    F: Fn(SandboxConfiguration) -> RawSandboxResult + Send + Sync + 'static,
{
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
                .unwrap();
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

    use pretty_assertions::assert_eq;
    use serde::{Deserialize, Serialize};
    use tempdir::TempDir;

    use task_maker_dag::*;

    use super::*;
    use rand::Rng;
    use tabox::result::{ExitStatus, ResourceUsage, SandboxExecutionResult};

    #[test]
    fn test_serialize_deserialize() {
        #[derive(Serialize, Deserialize)]
        struct Thing {
            pub x: u32,
            pub y: String,
        }

        let (tx, rx) = new_local_channel();
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
    fn test_remote_channels() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let mut server = ChannelServer::bind(("127.0.0.1", port)).unwrap();
        let client_thread = std::thread::spawn(move || {
            let (sender, receiver) = connect_channel(("127.0.0.1", port)).unwrap();
            sender.send(vec![1, 2, 3, 4]).unwrap();
            let data = receiver.recv().unwrap();
            assert_eq!(data, vec![5, 6, 7, 8]);
            sender.send(vec![9, 10, 11, 12]).unwrap();
        });

        let (sender, receiver, _addr) = server.next().unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
        sender.send(vec![5, 6, 7, 8]).unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![9, 10, 11, 12]);

        client_thread.join().unwrap();
    }

    fn fake_sandbox(config: SandboxConfiguration) -> RawSandboxResult {
        let resource_usage = ResourceUsage {
            memory_usage: 0,
            user_cpu_time: 0.0,
            system_cpu_time: 0.0,
            wall_time_usage: 0.0,
        };
        if config.executable.ends_with("true") {
            RawSandboxResult::Success(SandboxExecutionResult {
                status: ExitStatus::ExitCode(0),
                resource_usage,
            })
        } else {
            RawSandboxResult::Success(SandboxExecutionResult {
                status: ExitStatus::ExitCode(1),
                resource_usage,
            })
        }
    }

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

        eval_dag_locally(dag, cwd.path(), 2, cwd.path(), 1000, 1000, fake_sandbox);

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
