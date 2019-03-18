use crate::executor::*;
use crate::task_types::ioi::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::channel;

mod print;
mod raw;

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

    /// The generation of a testcase in a IOI task.
    IOIGeneration {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The status of the generation.
        status: UIExecutionStatus,
    },

    /// The validation of a testcase in a IOI task.
    IOIValidation {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The status of the validation.
        status: UIExecutionStatus,
    },

    /// The solution of a testcase in a IOI task.
    IOISolution {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The status of the solution.
        status: UIExecutionStatus,
    },

    /// The evaluation of a solution in a IOI task.
    IOIEvaluation {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The status of the solution.
        status: UIExecutionStatus,
    },

    /// The checking of a solution in a IOI task.
    IOIChecker {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The status of the solution. Note that a failure of this execution
        /// may not mean that the checker failed.
        status: UIExecutionStatus,
    },

    /// The score of a testcase is ready.
    IOITestcaseScore {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The id of the testcase.
        testcase: IOITestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The score of the testcase.
        score: f64,
    },

    /// The score of a subtask is ready.
    IOISubtaskScore {
        /// The id of the subtaks.
        subtask: IOISubtaskId,
        /// The path of the solution.
        solution: PathBuf,
        /// The score of the subtask.
        score: f64,
    },

    /// The score of a task is ready.
    IOITaskScore {
        /// The path of the solution.
        solution: PathBuf,
        /// The score of the task.
        score: f64,
    },
}

/// The sender of the UIMessage
pub struct UIMessageSender {
    sender: ChannelSender,
}

impl UIMessageSender {
    /// Make a new pair of UIMessageSender and ChannelReceiver.
    pub fn new() -> (UIMessageSender, ChannelReceiver) {
        // TODO: since this channel is always local to the client consider not
        // using the normal serializer and opt in using channel::<UIMessage>
        // directly.
        let (sender, receiver) = channel();
        (UIMessageSender { sender }, receiver)
    }

    /// Send a message to the channel.
    pub fn send(&self, message: UIMessage) -> Result<(), Error> {
        serialize_into(&message, &self.sender)
    }
}

/// The trait that describes the UI functionalities.
pub trait UI {
    /// Process a new UI message.
    fn on_message(&mut self, message: UIMessage);
}

/// The type of the UI to use, it enumerates all the known UI interfaces.
#[derive(Debug)]
pub enum UIType {
    /// The PrintUI.
    Print,
    /// The RawUI
    Raw,
}

impl UIType {
    pub fn start(&self, receiver: ChannelReceiver) {
        let mut ui: Box<dyn UI> = match self {
            UIType::Print => Box::new(print::PrintUI::new()),
            UIType::Raw => Box::new(raw::RawUI::new()),
        };
        while let Ok(message) = deserialize_from::<UIMessage>(&receiver) {
            ui.on_message(message);
        }
    }
}

impl std::str::FromStr for UIType {
    type Err = String;

    fn from_str(s: &str) -> Result<UIType, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "print" => Ok(UIType::Print),
            "raw" => Ok(UIType::Raw),
            _ => Err(format!("Unknown ui: {}", s)),
        }
    }
}
