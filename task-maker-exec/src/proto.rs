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
//! - `B` answers with `ProvideFile` which triggers a protocol switch, into FileProtocol
//! - `B` sends [`FileProtocol::Data`](enum.FileProtocol.html#variant.Data) zero or more times
//! - `B` sends [`FileProtocol::End`](enum.FileProtocol.html#variant.End) which triggers a protocol
//!   switch, back into normal mode

use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use task_maker_dag::*;
use task_maker_store::*;

/// Messages that the client sends to the server.
#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutorClientMessage {
    /// The client is asking to evaluate a DAG.
    Evaluate {
        /// The DAG to evaluate.
        dag: ExecutionDAGData,
        /// The list of the executions and files to keep track of.
        callbacks: ExecutionDAGWatchSet,
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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerClientMessage {
    /// The worker is ready for some job. The worker will wait for a
    /// [`Work`](enum.WorkerServerMessage.html#variant.Work) message.
    GetWork,
    /// The worker completed the job with this result producing those files. The actual files will
    /// be sent immediately after using `ProvideFile` messages.
    WorkerDone(ExecutionResult, HashMap<FileUuid, FileStoreKey>),
    /// The worker is sending a file to the server. After this message there is a protocol switch
    /// for the file transmission.
    ProvideFile(FileUuid, FileStoreKey),
    /// The worker needs a file from the server. The server should send back that file in order to
    /// run the execution on the worker.
    AskFile(FileStoreKey),
}

/// Messages sent by the server to the worker.
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerServerMessage {
    /// The job the worker should do. Boxed to reduce the enum size.
    Work(Box<WorkerJob>),
    /// The file the workers as asked. After this message there is a protocol switch for the file
    /// transmission.
    ProvideFile(FileStoreKey),
    /// Ask the worker to exit.
    Exit,
}

/// Messages sent during the FileProtocol operation, during the transfer of a file.
///
/// In this mode a series of `Data` messages is sent, ended by an `End` message. After this message
/// the protocol switches back to normal mode.
#[derive(Debug, Serialize, Deserialize)]
pub enum FileProtocol {
    /// A chunk of data.
    Data(Vec<u8>),
    /// The end of the stream.
    End,
}

/// An iterator over the byte chunks sent during the FileProtocol mode in a channel.
pub struct ChannelFileIterator<'a> {
    /// Reference to the channel from where to read
    reader: &'a ChannelReceiver,
}

impl<'a> ChannelFileIterator<'a> {
    /// Create a new iterator over a receiver channel.
    pub fn new(reader: &'a ChannelReceiver) -> ChannelFileIterator<'a> {
        ChannelFileIterator { reader }
    }
}

impl<'a> Iterator for ChannelFileIterator<'a> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        // errors cannot be handled in this iterator yet
        match deserialize_from::<FileProtocol>(self.reader).expect("deserialize error") {
            FileProtocol::Data(d) => Some(d),
            FileProtocol::End => None,
        }
    }
}

/// Utility function to send a file to a channel using [`FileProtocol`](enum.FileProtocol.html).
pub struct ChannelFileSender;

impl ChannelFileSender {
    /// Send a local file to a channel using [`FileProtocol`](enum.FileProtocol.html).
    pub fn send<P: AsRef<Path>>(path: P, sender: &ChannelSender) -> Result<(), Error> {
        for buf in ReadFileIterator::new(path.as_ref())? {
            serialize_into(&FileProtocol::Data(buf), sender)?;
        }
        serialize_into(&FileProtocol::End, sender)?;
        Ok(())
    }

    /// Send a file's data to a channel using [`FileProtocol`](enum.FileProtocol.html).
    pub fn send_data(data: Vec<u8>, sender: &ChannelSender) -> Result<(), Error> {
        serialize_into(&FileProtocol::Data(data), sender)?;
        serialize_into(&FileProtocol::End, sender)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_file() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        std::fs::write(tmpdir.path().join("file.txt"), "hello world").unwrap();

        let (sender, receiver) = std::sync::mpsc::channel();
        let receiver = ChannelFileIterator::new(&receiver);
        ChannelFileSender::send(tmpdir.path().join("file.txt"), &sender).unwrap();
        let data: Vec<u8> = receiver.flat_map(|d| d.into_iter()).collect();
        assert_eq!(String::from_utf8(data).unwrap(), "hello world");
    }

    #[test]
    fn test_send_content() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let receiver = ChannelFileIterator::new(&receiver);
        ChannelFileSender::send_data(b"hello world".to_vec(), &sender).unwrap();
        let data: Vec<u8> = receiver.flat_map(|d| d.into_iter()).collect();
        assert_eq!(String::from_utf8(data).unwrap(), "hello world");
    }
}
