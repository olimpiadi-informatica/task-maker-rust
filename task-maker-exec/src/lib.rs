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

#[macro_use]
extern crate log;
#[macro_use(defer)]
extern crate scopeguard;

use std::cell::RefCell;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use failure::{bail, Error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use client::ExecutorClient;
pub use executor::{ExecutorStatus, ExecutorWorkerStatus, WorkerCurrentJobStatus};
pub use sandbox::RawSandboxResult;
pub use sandbox_runner::{
    ErrorSandboxRunner, SandboxRunner, SuccessSandboxRunner, UnsafeSandboxRunner,
};
pub use scheduler::ClientInfo;
use task_maker_cache::Cache;
use task_maker_dag::ExecutionDAG;
use task_maker_store::FileStore;
pub use worker::{Worker, WorkerConn};

use crate::proto::FileProtocol;

mod check_dag;
mod client;
mod executor;
pub mod executors;
pub mod proto;
mod sandbox;
mod sandbox_runner;
mod scheduler;
mod worker;
mod worker_manager;

/// Message type that can be send in a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelMessage<T> {
    /// The message is a normal application message of type T.
    Message(T),
    /// The message encodes a `FileProtocol` message. This variant is only used in local channels.
    FileProtocol(FileProtocol),
    /// Message telling the other end that a file is coming of the specified length. This variant is
    /// only used in remote channels.
    RawFileData(usize),
    /// Message telling the other end that the file is ended, i.e. this was the last chunk. This
    /// variant is only used in remote channels.
    RawFileEnd,
}

/// The channel part that sends data.
#[derive(Debug, Clone)]
pub enum ChannelSender<T> {
    /// The connection is only a local in-memory channel.
    Local(Sender<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(Arc<Mutex<TcpStream>>),
}

/// The channel part that receives data.
#[derive(Debug)]
pub enum ChannelReceiver<T> {
    /// The connection is only a local in-memory channel.
    Local(Receiver<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(RefCell<TcpStream>),
}

impl<T> ChannelSender<T>
where
    T: 'static + Send + Sync + Serialize,
{
    /// Send some data into the channel.
    pub fn send(&self, data: T) -> Result<(), Error> {
        match self {
            ChannelSender::Local(sender) => sender
                .send(ChannelMessage::Message(data))
                .map_err(|e| e.into()),
            ChannelSender::Remote(sender) => {
                ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::Message(data))
            }
        }
    }

    /// Send some `FileProtocol` data in the channel.
    pub(crate) fn send_file(&self, data: FileProtocol) -> Result<(), Error> {
        match self {
            ChannelSender::Local(sender) => Ok(sender.send(ChannelMessage::FileProtocol(data))?),
            ChannelSender::Remote(sender) => match data {
                // Data is special, to avoid costly serialization of raw bytes, send the size of the
                // buffer and then the raw content.
                FileProtocol::Data(data) => {
                    ChannelSender::<T>::send_remote_raw(
                        sender,
                        ChannelMessage::RawFileData(data.len()),
                    )?;
                    let mut sender = sender.lock().expect("Cannot lock ChannelSender");
                    let stream = sender.deref_mut();
                    stream.write_all(&data).map_err(|e| e.into())
                }
                FileProtocol::End => {
                    ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::RawFileEnd)
                }
            },
        }
    }

    /// Send some unchecked data to the remote channel.
    fn send_remote_raw(
        sender: &Arc<Mutex<TcpStream>>,
        data: ChannelMessage<T>,
    ) -> Result<(), Error> {
        let mut sender = sender.lock().expect("Cannot lock ChannelSender");
        let stream = sender.deref_mut();
        bincode::serialize_into(stream, &data)?;
        Ok(())
    }

    /// Given this is a `ChannelSender::Remote`, change the type of the message. Will panic if this
    /// is a `ChannelSender::Local`.
    ///
    /// This function is useful for implementing a protocol where the message types change during
    /// the execution, for example because initially there is an handshake message, followed by the
    /// actual protocol messages.
    pub fn change_type<T2>(self) -> ChannelSender<T2> {
        match self {
            ChannelSender::Local(_) => panic!("Cannot change ChannelSender::Local type"),
            ChannelSender::Remote(r) => ChannelSender::Remote(r),
        }
    }
}

impl<'a, T> ChannelReceiver<T>
where
    T: 'static + DeserializeOwned,
{
    /// Receive a message from the channel.
    pub fn recv(&self) -> Result<T, Error> {
        let message = match self {
            ChannelReceiver::Local(receiver) => receiver.recv()?,
            ChannelReceiver::Remote(receiver) => ChannelReceiver::recv_remote_raw(receiver)?,
        };
        match message {
            ChannelMessage::Message(mex) => Ok(mex),
            _ => bail!("Expected ChannelMessage::Message"),
        }
    }

    /// Receive a message of the `FileProtocol` from the channel.
    pub(crate) fn recv_file(&self) -> Result<FileProtocol, Error> {
        match self {
            ChannelReceiver::Local(receiver) => match receiver.recv()? {
                ChannelMessage::FileProtocol(data) => Ok(data),
                _ => bail!("Expected ChannelMessage::FileProtocol"),
            },
            ChannelReceiver::Remote(receiver) => {
                match ChannelReceiver::<T>::recv_remote_raw(receiver)? {
                    ChannelMessage::RawFileData(len) => {
                        let mut receiver = receiver.borrow_mut();
                        let mut buf = vec![0u8; len];
                        receiver.read_exact(&mut buf)?;
                        Ok(FileProtocol::Data(buf))
                    }
                    ChannelMessage::RawFileEnd => Ok(FileProtocol::End),
                    _ => {
                        bail!("Expected ChannelMessage::RawFileData or ChannelMessage::RawFileEnd")
                    }
                }
            }
        }
    }

    /// Receive a message from the TCP stream of a channel.
    fn recv_remote_raw(receiver: &RefCell<TcpStream>) -> Result<ChannelMessage<T>, Error> {
        let mut receiver = receiver.borrow_mut();
        Ok(bincode::deserialize_from(receiver.deref_mut())?)
    }

    /// Given this is a `ChannelReceiver::Remote`, change the type of the message. Will panic if
    /// this is a `ChannelReceiver::Local`.
    ///
    /// This function is useful for implementing a protocol where the message types change during
    /// the execution, for example because initially there is an handshake message, followed by the
    /// actual protocol messages.
    pub fn change_type<T2>(self) -> ChannelReceiver<T2> {
        match self {
            ChannelReceiver::Local(_) => panic!("Cannot change ChannelReceiver::Local type"),
            ChannelReceiver::Remote(r) => ChannelReceiver::Remote(r),
        }
    }
}

/// Make a new pair of `ChannelSender` / `ChannelReceiver`
pub fn new_local_channel<T>() -> (ChannelSender<T>, ChannelReceiver<T>) {
    let (tx, rx) = unbounded();
    (ChannelSender::Local(tx), ChannelReceiver::Local(rx))
}

/// Listener for connections on some TCP socket.
///
/// `S` and `R` are the types of message sent and received respectively.
pub struct ChannelServer<S, R>(TcpListener, PhantomData<*const S>, PhantomData<*const R>);

impl<S, R> ChannelServer<S, R> {
    /// Bind a socket and create a new `ChannelServer`.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer(
            TcpListener::bind(addr)?,
            PhantomData::default(),
            PhantomData::default(),
        ))
    }
}

impl<S, R> Deref for ChannelServer<S, R> {
    type Target = TcpListener;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, R> Iterator for ChannelServer<S, R> {
    type Item = (ChannelSender<S>, ChannelReceiver<R>, SocketAddr);

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
pub fn connect_channel<A: ToSocketAddrs, T>(
    addr: A,
) -> Result<(ChannelSender<T>, ChannelReceiver<T>), Error> {
    let stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;
    Ok((
        ChannelSender::Remote(Arc::new(Mutex::new(stream))),
        ChannelReceiver::Remote(RefCell::new(stream2)),
    ))
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
    use rand::Rng;
    use serde::{Deserialize, Serialize};
    use tempdir::TempDir;

    use task_maker_dag::*;

    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        #[derive(Serialize, Deserialize)]
        struct Thing {
            pub x: u32,
            pub y: String,
        }

        let (tx, rx) = new_local_channel();
        tx.send(Thing {
            x: 42,
            y: "foobar".into(),
        })
        .unwrap();
        let thing: Thing = rx.recv().unwrap();
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
        let data: Vec<i32> = receiver.recv().unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
        sender.send(vec![5, 6, 7, 8]).unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![9, 10, 11, 12]);

        client_thread.join().unwrap();
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
