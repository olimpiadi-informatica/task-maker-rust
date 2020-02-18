//! The UI functionality for the task formats.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

use failure::Error;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorSpec, StandardStream};

pub use json::JsonUI;
pub use print::PrintUI;
pub use raw::RawUI;
pub use silent::SilentUI;
use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult, ExecutionStatus, WorkerUuid};
pub use ui_message::UIMessage;

use crate::{cwrite, cwriteln};

mod json;
mod print;
mod raw;
mod silent;
mod ui_message;

/// Channel type for sending `UIMessage`s.
pub type UIChannelSender = Sender<UIMessage>;
/// Channel type for receiving `UIMessage`s.
pub type UIChannelReceiver = Receiver<UIMessage>;

lazy_static! {
    /// The RED color to use with `cwrite!` and `cwriteln!`
    pub static ref RED: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Red))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The GREEN color to use with `cwrite!` and `cwriteln!`
    pub static ref GREEN: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Green))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The YELLOW color to use with `cwrite!` and `cwriteln!`
    pub static ref YELLOW: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Yellow))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The BLUE color to use with `cwrite!` and `cwriteln!`
    pub static ref BLUE: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Blue))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The bold style to use with `cwrite!` and `cwriteln!`
    pub static ref BOLD: ColorSpec = {
        let mut color = ColorSpec::new();
        color.set_bold(true);
        color
    };
}

/// The status of an execution.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

/// The status of the compilation of a file.
#[derive(Debug, Clone, PartialEq)]
pub enum CompilationStatus {
    /// The compilation is known but it has not started yet.
    Pending,
    /// The compilation is running on a worker.
    Running,
    /// The compilation has completed.
    Done {
        /// The result of the compilation.
        result: ExecutionResult,
        /// The standard output of the compilation.
        stdout: Option<String>,
        /// The standard error of the compilation.
        stderr: Option<String>,
    },
    /// The compilation has failed.
    Failed {
        /// The result of the compilation.
        result: ExecutionResult,
        /// The standard output of the compilation.
        stdout: Option<String>,
        /// The standard error of the compilation.
        stderr: Option<String>,
    },
    /// The compilation has been skipped.
    Skipped,
}

impl CompilationStatus {
    /// Apply to this `CompilationStatus` a new `UIExecutionStatus`.
    pub fn apply_status(&mut self, status: UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => *self = CompilationStatus::Pending,
            UIExecutionStatus::Started { .. } => *self = CompilationStatus::Running,
            UIExecutionStatus::Done { result } => {
                if let ExecutionStatus::Success = result.status {
                    *self = CompilationStatus::Done {
                        result,
                        stdout: None,
                        stderr: None,
                    };
                } else {
                    *self = CompilationStatus::Failed {
                        result,
                        stdout: None,
                        stderr: None,
                    };
                }
            }
            UIExecutionStatus::Skipped => *self = CompilationStatus::Skipped,
        }
    }

    /// Set the standard output of the compilation.
    pub fn apply_stdout(&mut self, content: String) {
        // FIXME: if the stdout is sent before the status of the execution this breaks
        match self {
            CompilationStatus::Done { stdout, .. } | CompilationStatus::Failed { stdout, .. } => {
                stdout.replace(content);
            }
            _ => {}
        }
    }

    /// Set the standard error of the compilation.
    pub fn apply_stderr(&mut self, content: String) {
        match self {
            CompilationStatus::Done { stderr, .. } | CompilationStatus::Failed { stderr, .. } => {
                stderr.replace(content);
            }
            _ => {}
        }
    }
}

/// Collection of utilities for drawing the finish UI.
pub struct FinishUIUtils<'a> {
    /// Stream where to print to.
    stream: &'a mut StandardStream,
}

impl<'a> FinishUIUtils<'a> {
    /// Make a new `FinishUIUtils` borrowing a `StandardStream`.
    pub fn new(stream: &'a mut StandardStream) -> FinishUIUtils<'a> {
        FinishUIUtils { stream }
    }

    /// Print all the compilation statuses.
    pub fn print_compilations(&mut self, compilations: &HashMap<PathBuf, CompilationStatus>) {
        cwriteln!(self, BLUE, "Compilations");
        let max_len = compilations
            .keys()
            .map(|p| p.file_name().expect("Invalid file name").len())
            .max()
            .unwrap_or(0);
        for (path, status) in compilations {
            print!(
                "{:width$}  ",
                path.file_name()
                    .expect("Invalid file name")
                    .to_string_lossy(),
                width = max_len
            );
            match status {
                CompilationStatus::Done { result, .. } => {
                    cwrite!(self, GREEN, " OK  ");
                    FinishUIUtils::print_time_memory(&result.resources);
                }
                CompilationStatus::Failed {
                    result,
                    stdout,
                    stderr,
                } => {
                    cwrite!(self, RED, "FAIL ");
                    FinishUIUtils::print_time_memory(&result.resources);
                    if let Some(stdout) = stdout {
                        if !stdout.trim().is_empty() {
                            println!();
                            cwriteln!(self, BOLD, "stdout:");
                            println!("{}", stdout.trim());
                        }
                    }
                    if let Some(stderr) = stderr {
                        if !stderr.trim().is_empty() {
                            println!();
                            cwriteln!(self, BOLD, "stderr:");
                            println!("{}", stderr.trim());
                        }
                    }
                }
                _ => {
                    cwrite!(self, YELLOW, "{:?}", status);
                }
            }
            println!();
        }
    }

    /// Print the time and memory usage of an execution.
    pub fn print_time_memory(resources: &ExecutionResourcesUsage) {
        print!(
            "{:2.3}s | {:3.1}MiB",
            resources.cpu_time,
            (resources.memory as f64) / 1024.0
        );
    }

    /// Print a message for the non-successful variants of the provided status.
    pub fn print_fail_execution_status(status: &ExecutionStatus) {
        match status {
            ExecutionStatus::Success => {}
            ExecutionStatus::ReturnCode(code) => print!("Exited with {}", code),
            ExecutionStatus::Signal(sig, name) => print!("Signal {} ({})", sig, name),
            ExecutionStatus::TimeLimitExceeded => print!("Time limit exceeded"),
            ExecutionStatus::SysTimeLimitExceeded => print!("Kernel time limit exceeded"),
            ExecutionStatus::WallTimeLimitExceeded => print!("Wall time limit exceeded"),
            ExecutionStatus::MemoryLimitExceeded => print!("Memory limit exceeded"),
            ExecutionStatus::InternalError(err) => print!("Internal error: {}", err),
        }
    }

    /// Find the maximum length of the solutions name from the keys of the given structure.
    pub fn get_max_len<T>(solutions: &HashMap<PathBuf, T>) -> usize {
        solutions
            .keys()
            .map(|p| p.file_name().expect("Invalid file name").len())
            .max()
            .unwrap_or(0)
    }
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
    /// The `SilentUI`.
    Silent,
}

impl std::str::FromStr for UIType {
    type Err = String;

    fn from_str(s: &str) -> Result<UIType, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "print" => Ok(UIType::Print),
            "raw" => Ok(UIType::Raw),
            "curses" => Ok(UIType::Curses),
            "json" => Ok(UIType::Json),
            "silent" => Ok(UIType::Silent),
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
