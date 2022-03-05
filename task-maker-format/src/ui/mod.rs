//! The UI functionality for the task formats.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

use anyhow::Error;
use serde::{Deserialize, Serialize};
pub use termcolor::WriteColor;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream};
use typescript_definitions::TypeScriptify;

pub use json::JsonUI;
pub use print::PrintUI;
pub use raw::RawUI;
pub use silent::SilentUI;
use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult, ExecutionStatus, WorkerUuid};
pub use ui_message::UIMessage;

use crate::{cwrite, cwriteln};

pub(crate) mod curses;
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
    /// Whether the terminal supports ANSI 256 colors.
    static ref HAS_256: bool = {
        if std::env::var("TM_ANSI256").as_deref() == Ok("true") {
            if let Some(support) = supports_color::on(supports_color::Stream::Stdout) {
                support.has_256
            } else {
                false
            }
        } else {
            false
        }
    };
    /// The RED color to use with `cwrite!` and `cwriteln!`
    pub static ref RED: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(196) } else { Color::Red }))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The RED color to use with `cwrite!` and `cwriteln!`, without bold.
    pub static ref SOFT_RED: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(196) } else { Color::Red }))
            .set_intense(true);
        color
    };
    /// The GREEN color to use with `cwrite!` and `cwriteln!`
    pub static ref GREEN: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(118) } else { Color::Green }))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The YELLOW color to use with `cwrite!` and `cwriteln!`
    pub static ref YELLOW: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(226) } else { Color::Yellow }))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The ORANGE color to use with `cwrite!` and `cwriteln!`.
    pub static ref ORANGE: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(214) } else { Color::Rgb(255, 165, 0) }))
            .set_intense(true)
            .set_bold(true);
        color
    };
    /// The BLUE color to use with `cwrite!` and `cwriteln!`
    pub static ref BLUE: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(if *HAS_256 { Color::Ansi256(33) } else { Color::Blue }))
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
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TypeScriptify)]
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
                let stdout = result
                    .stdout
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).into());
                let stderr = result
                    .stderr
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).into());
                if let ExecutionStatus::Success = result.status {
                    *self = CompilationStatus::Done {
                        result,
                        stdout,
                        stderr,
                    };
                } else {
                    *self = CompilationStatus::Failed {
                        result,
                        stdout,
                        stderr,
                    };
                }
            }
            UIExecutionStatus::Skipped => *self = CompilationStatus::Skipped,
        }
    }
}

/// The state of a task, all the information for the UI are stored here.
///
/// The `T` at the end is to disambiguate from `UIState` due to a strange behaviour of the compiler.
pub trait UIStateT {
    /// Apply a `UIMessage` to this state.
    fn apply(&mut self, message: UIMessage);

    /// Print the final results using a finish UI.
    fn finish(&mut self);
}

/// UI that prints to `stdout` the ending result of the evaluation of a task.
pub trait FinishUI<State> {
    /// Print the final state of the UI.
    fn print(state: &State);
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

    /// Print the warnings.
    pub fn print_warning_messages(&mut self, warnings: &[String]) {
        if !warnings.is_empty() {
            cwriteln!(self, YELLOW, "Warnings");
            for warning in warnings.iter() {
                println!(" - {}", warning);
            }
            println!();
        }
    }

    /// Print the errors.
    pub fn print_error_messages(&mut self, errors: &[String]) {
        if !errors.is_empty() {
            cwriteln!(self, RED, "Errors");
            for error in errors.iter() {
                cwrite!(self, RED, " - ");
                cwriteln!(self, SOFT_RED, "{}", error);
            }
            println!();
        }
    }
}

/// The sender of the UIMessage
#[derive(Debug)]
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

    /// Send a warning message to the channel.
    pub fn send_warning(&self, message: impl Into<String>) -> Result<(), Error> {
        self.send(UIMessage::Warning {
            message: message.into(),
        })
    }

    /// Send an error message to the channel.
    pub fn send_error(&self, message: impl Into<String>) -> Result<(), Error> {
        self.send(UIMessage::Error {
            message: message.into(),
        })
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
#[derive(Debug, Clone)]
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

/// A simple printer that outputs to stdout. This can be used with `cwrite!` and `cwriteln!`.
#[allow(dead_code)]
pub struct StdoutPrinter {
    /// The actual stream.
    pub stream: StandardStream,
}

impl Default for StdoutPrinter {
    fn default() -> Self {
        Self {
            stream: StandardStream::stdout(ColorChoice::Auto),
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
/// use termcolor::{ColorSpec, ColorChoice};
/// use task_maker_format::ui::StdoutPrinter;
/// use task_maker_format::cwrite;
///
/// # fn main() {
/// let mut color = ColorSpec::new();
/// color.set_bold(true);
///
/// let mut printer = StdoutPrinter::default();
/// cwrite!(printer, color, "The output is {}", 42);
/// # }
/// ```
#[macro_export]
macro_rules! cwrite {
    ($self:expr, $color:expr, $($arg:tt)*) => {{
        use std::io::Write;
        use $crate::ui::WriteColor;
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
/// use termcolor::{ColorSpec, ColorChoice};
/// use task_maker_format::ui::StdoutPrinter;
/// use task_maker_format::cwrite;
///
/// # fn main() {
/// let mut color = ColorSpec::new();
/// color.set_bold(true);
///
/// let mut printer = StdoutPrinter::default();
/// cwriteln!(printer, color, "The output is {}", 42);
/// # }
/// ```
#[macro_export]
macro_rules! cwriteln {
    ($self:expr, $color:expr, $($arg:tt)*) => {{
        use std::io::Write;
        use $crate::ui::WriteColor;
        $self.stream.set_color(&$color).unwrap();
        writeln!(&mut $self.stream, $($arg)*).unwrap();
        $self.stream.reset().unwrap();
    }};
}
