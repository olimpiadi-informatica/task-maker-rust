use crate::executor::*;
use crate::task_types::ioi::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

mod curses;
mod ioi_state;
mod print;
mod raw;

pub type UIChannelSender = Sender<UIMessage>;
pub type UIChannelReceiver = Receiver<UIMessage>;

/// The status of an execution.
#[derive(Debug, Serialize, Deserialize, Clone)]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UIMessage {
    /// An update on the compilation status.
    Compilation {
        /// The compilation of this file.
        file: PathBuf,
        /// The status of the compilation.
        status: UIExecutionStatus,
    },

    /// The information about the task which is being run.
    IOITask {
        /// The short name of the task.
        name: String,
        /// The long name of the task.
        title: String,
        /// The path to the task on the client disk.
        path: PathBuf,
        // TODO: time/mem limits
        /// The list of the subtasks with their information.
        subtasks: HashMap<IOISubtaskId, IOISubtaskInfo>,
        /// The set of testcases for each subtask.
        testcases: HashMap<IOISubtaskId, HashSet<IOITestcaseId>>,
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
    sender: UIChannelSender,
}

impl UIMessageSender {
    /// Make a new pair of UIMessageSender and ChannelReceiver.
    pub fn new() -> (UIMessageSender, UIChannelReceiver) {
        let (sender, receiver) = channel();
        (UIMessageSender { sender }, receiver)
    }

    /// Send a message to the channel.
    pub fn send(&self, message: UIMessage) -> Result<(), Error> {
        self.sender.send(message).map_err(|e| e.into())
    }
}

/// The trait that describes the UI functionalities.
pub trait UI {
    /// Process a new UI message.
    fn on_message(&mut self, message: UIMessage);
    /// Make the UI print the ending results.
    fn finish(&mut self);
}

/// The type of the UI to use, it enumerates all the known UI interfaces.
#[derive(Debug)]
pub enum UIType {
    /// The PrintUI.
    Print,
    /// The RawUI
    Raw,
    /// The CursesUI
    Curses,
}

impl UIType {
    pub fn start(&self, receiver: UIChannelReceiver) {
        let mut ui: Box<dyn UI> = match self {
            UIType::Print => Box::new(print::PrintUI::new()),
            UIType::Raw => Box::new(raw::RawUI::new()),
            UIType::Curses => Box::new(curses::CursesUI::new()),
        };
        while let Ok(message) = receiver.recv() {
            ui.on_message(message);
        }
        ui.finish();
    }
}

impl std::str::FromStr for UIType {
    type Err = String;

    fn from_str(s: &str) -> Result<UIType, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "print" => Ok(UIType::Print),
            "raw" => Ok(UIType::Raw),
            "curses" => Ok(UIType::Curses),
            _ => Err(format!("Unknown ui: {}", s)),
        }
    }
}
