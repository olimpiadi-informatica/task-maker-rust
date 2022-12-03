//! The protocol related structs and enums.
//!
//! # Protocol Specification
//!
//! The communication between the services is done using the abstraction of
//! `std::sync::mpsc::channel`. The messages are serialized and then sent inside a `channel`. The
//! way the messages are serialized is unspecified and
//! [`serialize_into`](../../task_maker_exec/fn.serialize_into.html)/[`deserialize_from`](../../task_maker_exec/fn.deserialize_from.html)
//! should be used instead of reading from / writing to the channels.
//!
//! There are 3 basic actors in the protocol:
//!
//! - The Client: the one who makes the DAG and is interested in the results;
//! - The Server: the one who manages the execution of the DAGs;
//! - The Worker: the one who is able to run an `Execution` inside a `Sandbox`.
//!
//! The clients are able to communicate to the server, the workers are able to communicate to the
//! server and the server is able to communicate with both. The clients and the workers should not
//! communicate directly.
//!
//! The 4 valid communication directions are:
//! - `Client` — [`ExecutorClientMessage`](enum.ExecutorClientMessage.html) → `Server`
//! - `Client` ← [`ExecutorServerMessage`](enum.ExecutorServerMessage.html) — `Server`
//! - `Worker` — [`WorkerClientMessage`](enum.WorkerClientMessage.html) → `Server`
//! - `Worker` ← [`WorkerServerMessage`](enum.WorkerServerMessage.html) — `Server`
//!
//! When an actor needs a file a particular series of messages is sent. Let's assume `A` wants a
//! file from `B`:
//! - `A` sends a `AskFile` to `B`
//! - `B` answers with `ProvideFile` which triggers a protocol switch for sending the file
//! - `B` sends raw data (`send_raw`) zero or more times
//! - `B` sends empty raw data which triggers a protocol switch, back into normal mode

use crate::executor::{ExecutionDAGWatchSet, ExecutorStatus, WorkerJob};
use crate::*;
use anyhow::Context;
use ductile::{ChannelReceiver, ChannelSender};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use task_maker_dag::*;
use task_maker_store::*;

/// Messages that the client sends to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutorClientMessage {
    /// The client is asking to evaluate a DAG.
    Evaluate {
        /// The DAG to evaluate.
        dag: Box<ExecutionDAGData>,
        /// The list of the executions and files to keep track of.
        callbacks: Box<ExecutionDAGWatchSet>,
    },
    /// The client is providing a file. After this message there is a protocol switch for the file
    /// transmission.
    ProvideFile(FileUuid, FileStoreKey),
    /// The client is asking the server to send a file. After this message there is a protocol
    /// switch for the file transmission.
    AskFile(FileUuid, FileStoreKey, bool),
    /// The client is asking to stop the evaluation. All the running executions will be killed and
    /// no more execution will be run. All the callbacks will be called as usual.
    Stop,
    /// The client is asking for the server status. After this message the client should expect a
    /// [`Status`](enum.ExecutorServerMessage.html#variant.Status) message back.
    Status,
}

/// Messages that the server sends to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutorServerMessage {
    /// The server needs the file with that Uuid. The client must send back that file in order to
    /// proceed with the execution.
    AskFile(FileUuid),
    /// The server is sending a file. After this message there is a protocol switch for the file
    /// transmission protocol. The second entry is true if the generation of the file was
    /// successful.
    ProvideFile(FileUuid, bool),
    /// The execution has started on a worker.
    NotifyStart(ExecutionUuid, WorkerUuid),
    /// The execution has completed with that result.
    NotifyDone(ExecutionUuid, ExecutionResult),
    /// The execution has been skipped.
    NotifySkip(ExecutionUuid),
    /// There was an error during the evaluation.
    Error(String),
    /// The server status as asked by the client.
    Status(ExecutorStatus<Duration>),
    /// The evaluation of the DAG is complete, this message will close the connection.
    Done(Vec<(FileUuid, FileStoreKey, bool)>),
}

/// Messages sent by the workers to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerClientMessage {
    /// The worker is ready for some job. The worker will wait for a
    /// [`Work`](enum.WorkerServerMessage.html#variant.Work) message.
    GetWork,
    /// The worker completed the job with this result producing those files. The actual files will
    /// be sent immediately after using `ProvideFile` messages.
    /// The list of `ExecutionResult` contains the results of all the executions inside the group,
    /// in the same order.
    WorkerDone(Vec<ExecutionResult>, HashMap<FileUuid, FileStoreKey>),
    /// The worker is sending a file to the server. After this message there is a protocol switch
    /// for the file transmission.
    ProvideFile(FileUuid, FileStoreKey),
    /// The worker needs a file from the server. The server should send back that file in order to
    /// run the execution on the worker.
    AskFile(FileStoreKey),
}

/// Messages sent by the server to the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerServerMessage {
    /// The job the worker should do. Boxed to reduce the enum size.
    Work(Box<WorkerJob>),
    /// Stop the current worker sandbox if currently running the specified execution.
    KillJob(ExecutionGroupUuid),
    /// The file the workers as asked. After this message there is a protocol switch for the file
    /// transmission.
    ProvideFile(FileStoreKey),
    /// The worker completed the execution and produced some files, the server asks the ones that
    /// are missing using this message.
    AskFiles(Vec<FileUuid>),
    /// Ask the worker to exit.
    Exit,
}

/// An iterator over the byte chunks sent during the file transfer mode in a channel.
pub struct ChannelFileIterator<'a, T>
where
    T: Send + Sync + DeserializeOwned,
{
    /// Reference to the channel from where to read
    reader: &'a ChannelReceiver<T>,
}

impl<'a, T> ChannelFileIterator<'a, T>
where
    T: 'static + Send + Sync + DeserializeOwned,
{
    /// Create a new iterator over a receiver channel.
    pub fn new(reader: &'a ChannelReceiver<T>) -> ChannelFileIterator<'a, T> {
        ChannelFileIterator { reader }
    }
}

impl<'a, T> Iterator for ChannelFileIterator<'a, T>
where
    T: 'static + Send + Sync + DeserializeOwned,
{
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        // errors cannot be handled in this iterator yet
        let data = self.reader.recv_raw().expect("deserialize error");
        if data.is_empty() {
            None
        } else {
            Some(data)
        }
    }
}

/// Utility function to send a file to a channel using [`send_raw`](https://docs.rs/ductile/0.1.0/ductile/struct.ChannelSender.html#method.send_raw).
pub struct ChannelFileSender;

impl ChannelFileSender {
    /// Send a local file to a channel using `send_raw`.
    pub fn send<P: AsRef<Path>, T>(path: P, sender: &ChannelSender<T>) -> Result<(), Error>
    where
        T: 'static + Send + Sync + Serialize,
    {
        let path = path.as_ref();
        let iterator = ReadFileIterator::new(path)
            .with_context(|| format!("Failed to read file to send: {}", path.display()))?;
        for buf in iterator {
            sender.send_raw(&buf).context("Failed to send file chunk")?;
        }
        sender
            .send_raw(&[])
            .context("Failed to send file terminator")?;
        Ok(())
    }

    /// Send the file content to a channel using `send_raw`.
    pub fn send_data<T>(data: Vec<u8>, sender: &ChannelSender<T>) -> Result<(), Error>
    where
        T: 'static + Send + Sync + Serialize,
    {
        sender
            .send_raw(&data)
            .context("Failed to send file chunk")?;
        // Send the EOF chunk only if the buffer is not empty (otherwise we would send EOF twice
        // breaking the protocol).
        if !data.is_empty() {
            sender
                .send_raw(&[])
                .context("Failed to send file terminator")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_file() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        std::fs::write(tmpdir.path().join("file.txt"), "hello world").unwrap();

        let (sender, receiver) = new_local_channel::<()>();
        let receiver = ChannelFileIterator::new(&receiver);
        ChannelFileSender::send(tmpdir.path().join("file.txt"), &sender).unwrap();
        let data: Vec<u8> = receiver.flat_map(|d| d.into_iter()).collect();
        assert_eq!(String::from_utf8(data).unwrap(), "hello world");
    }

    #[test]
    fn test_send_content() {
        let (sender, receiver) = new_local_channel::<()>();
        let receiver = ChannelFileIterator::new(&receiver);
        ChannelFileSender::send_data(b"hello world".to_vec(), &sender).unwrap();
        let data: Vec<u8> = receiver.flat_map(|d| d.into_iter()).collect();
        assert_eq!(String::from_utf8(data).unwrap(), "hello world");
    }
}
