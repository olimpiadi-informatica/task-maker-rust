use crate::executor::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::channel;

/// The status of an execution.
#[derive(Debug, Serialize, Deserialize)]
pub enum UIExecutionStatus {
    /// The Execution is known to the DAG and when all its dependencies are
    /// ready it will be started.
    Pending,
    /// The Execution has been started on a worker.
    Started { worker: String },
    /// The Execution has been completed.
    Done { result: WorkerResult },
    /// At least one of its dependencies have failed, the Execution has been
    /// skipped.
    Skipped,
}

/// A message sent to the UI.
#[derive(Debug, Serialize, Deserialize)]
pub enum UIMessage {
    /// An update on the compilation status.
    Compilation {
        /// The compilation of this file.
        file: PathBuf,
        /// The status of the compilation.
        status: UIExecutionStatus,
    },
}

/// The sender of the UIMessage
pub struct UIMessageSender {
    sender: ChannelSender,
}

impl UIMessageSender {
    /// Make a new pair of UIMessageSender and ChannelReceiver.
    pub fn new() -> (UIMessageSender, ChannelReceiver) {
        let (sender, receiver) = channel();
        (UIMessageSender { sender }, receiver)
    }

    /// Send a message to the channel.
    pub fn send(&self, message: UIMessage) -> Result<(), Error> {
        serialize_into(&message, &self.sender)
    }
}
