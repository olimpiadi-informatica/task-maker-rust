//! The UI functionality for the task formats.

use crate::ioi::*;
use task_maker_dag::{ExecutionResult, WorkerUuid};

use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

mod json;
mod print;
mod raw;

pub use json::JsonUI;
pub use print::PrintUI;
pub use raw::RawUI;

/// Channel type for sending `UIMessage`s.
pub type UIChannelSender = Sender<UIMessage>;
/// Channel type for receiving `UIMessage`s.
pub type UIChannelReceiver = Receiver<UIMessage>;

/// The status of an execution.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UIExecutionStatus {
    /// The `Execution` is known to the DAG and when all its dependencies are ready it will
    /// started.
    Pending,
    /// The `Execution` has been started on a worker.
    Started {
        /// The UUID of the worker.
        worker: WorkerUuid,
    },
    /// The `Execution` has been completed.
    Done {
        /// The result of the execution.
        result: ExecutionResult,
    },
    /// At least one of its dependencies have failed, the `Execution` has been skipped.
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
        /// The task information.
        task: Task,
    },

    /// The generation of a testcase in a IOI task.
    IOIGeneration {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The status of the generation.
        status: UIExecutionStatus,
    },

    /// The validation of a testcase in a IOI task.
    IOIValidation {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The status of the validation.
        status: UIExecutionStatus,
    },

    /// The solution of a testcase in a IOI task.
    IOISolution {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The status of the solution.
        status: UIExecutionStatus,
    },

    /// The evaluation of a solution in a IOI task.
    IOIEvaluation {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The status of the solution.
        status: UIExecutionStatus,
    },

    /// The checking of a solution in a IOI task.
    IOIChecker {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The status of the solution. Note that a failure of this execution
        /// may not mean that the checker failed.
        status: UIExecutionStatus,
    },

    /// The score of a testcase is ready.
    IOITestcaseScore {
        /// The id of the subtask.
        subtask: SubtaskId,
        /// The id of the testcase.
        testcase: TestcaseId,
        /// The path of the solution.
        solution: PathBuf,
        /// The score of the testcase.
        score: f64,
        /// The message associated with the score.
        message: String,
    },

    /// The score of a subtask is ready.
    IOISubtaskScore {
        /// The id of the subtask.
        subtask: SubtaskId,
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
pub trait UI: Send {
    /// Process a new UI message.
    fn on_message(&mut self, message: UIMessage);
    /// Make the UI print the ending results.
    fn finish(&mut self);
}

/// The type of the UI to use, it enumerates all the known UI interfaces.
#[derive(Debug)]
pub enum UIType {
    /// The `PrintUI`.
    Print,
    /// The `RawUI`.
    Raw,
    /// The `CursesUI`.
    Curses,
    /// The `JsonUI`.
    Json,
}

impl std::str::FromStr for UIType {
    type Err = String;

    fn from_str(s: &str) -> Result<UIType, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "print" => Ok(UIType::Print),
            "raw" => Ok(UIType::Raw),
            "curses" => Ok(UIType::Curses),
            "json" => Ok(UIType::Json),
            _ => Err(format!("Unknown ui: {}", s)),
        }
    }
}

/// Write to `$self.stream`, in the color specified as second parameter. The arguments that follow
/// will be passed to `write!`.
///
/// ```
/// #[macro_use]
/// extern crate task_maker_format;
///
/// use termcolor::{StandardStream, ColorSpec, ColorChoice};
/// use task_maker_format::cwrite;
///
/// # fn main() {
/// struct Printer { stream: StandardStream }
/// let mut color = ColorSpec::new();
/// color.set_bold(true);
///
/// let mut printer = Printer { stream: StandardStream::stdout(ColorChoice::Auto) };
/// cwrite!(printer, color, "The output is {}", 42);
/// # }
/// ```
#[macro_export]
macro_rules! cwrite {
    ($self:expr, $color:expr, $($arg:tt)*) => {{
        use termcolor::WriteColor;
        use std::io::Write;
        $self.stream.set_color(&$color).unwrap();
        write!(&mut $self.stream, $($arg)*).unwrap();
        $self.stream.reset().unwrap();
    }};
}

/// Write to `$self.stream`, in the color specified as second parameter. The arguments that follow
/// will be passed to `writeln!`.
///
/// ```
/// #[macro_use]
/// extern crate task_maker_format;
///
/// use termcolor::{StandardStream, ColorSpec, ColorChoice};
/// use task_maker_format::cwriteln;
///
/// # fn main() {
/// struct Printer { stream: StandardStream }
/// let mut color = ColorSpec::new();
/// color.set_bold(true);
///
/// let mut printer = Printer { stream: StandardStream::stdout(ColorChoice::Auto) };
/// cwriteln!(printer, color, "The output is {}", 42);
/// # }
/// ```
#[macro_export]
macro_rules! cwriteln {
    ($self:expr, $color:expr, $($arg:tt)*) => {{
        use termcolor::WriteColor;
        use std::io::Write;
        $self.stream.set_color(&$color).unwrap();
        writeln!(&mut $self.stream, $($arg)*).unwrap();
        $self.stream.reset().unwrap();
    }};
}
