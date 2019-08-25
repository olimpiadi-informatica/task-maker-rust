use crate::cwrite;
use crate::ioi::finish_ui::FinishUI;
use crate::ioi::ui_state::UIState;
use crate::ioi::Task;
use crate::ui::*;
use itertools::Itertools;
use task_maker_dag::ExecutionStatus;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream};

lazy_static! {
    static ref ERROR: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Red))
            .set_intense(true)
            .set_bold(true);
        color
    };
    static ref SUCCESS: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Green))
            .set_intense(true)
            .set_bold(true);
        color
    };
    static ref WARNING: ColorSpec = {
        let mut color = ColorSpec::new();
        color
            .set_fg(Some(Color::Yellow))
            .set_intense(true)
            .set_bold(true);
        color
    };
    static ref BOLD: ColorSpec = {
        let mut color = ColorSpec::new();
        color.set_bold(true);
        color
    };
}

/// A simple UI that will print to stdout the human readable messages. Useful
/// for debugging or for when curses is not available.
pub struct PrintUI {
    stream: StandardStream,
    state: UIState,
}

impl PrintUI {
    /// Make a new PrintUI.
    pub fn new(task: &Task) -> PrintUI {
        PrintUI {
            stream: StandardStream::stdout(ColorChoice::Auto),
            state: UIState::new(task),
        }
    }

    /// Write the UIExecutionStatus type to the console, coloring the message.
    fn write_status(&mut self, status: &UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => print!("[PENDING] "),
            UIExecutionStatus::Started { .. } => print!("[STARTED] "),
            UIExecutionStatus::Done { result } => match result.status {
                ExecutionStatus::Success => cwrite!(self, SUCCESS, "[DONE]    "),
                _ => cwrite!(self, WARNING, "[DONE]    "),
            },
            UIExecutionStatus::Skipped => cwrite!(self, WARNING, "[SKIPPED] "),
        };
    }

    /// Write the UIExecutionStatus details to the console.
    fn write_status_details(&mut self, status: &UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => {}
            UIExecutionStatus::Started { worker } => {
                print!("Worker: {:?}", worker);
            }
            UIExecutionStatus::Done { result } => {
                self.write_execution_status(&result.status);
            }
            UIExecutionStatus::Skipped => {}
        }
    }

    /// Write the ExecutionStatus details to the console.
    fn write_execution_status(&mut self, status: &ExecutionStatus) {
        match status {
            ExecutionStatus::Success => cwrite!(self, SUCCESS, "[{:?}]", status),
            ExecutionStatus::InternalError(_) => cwrite!(self, ERROR, "[{:?}]", status),
            _ => cwrite!(self, WARNING, "[{:?}]", status),
        }
    }

    /// Write a message, padding it to at least 80 chars.
    fn write_message(&mut self, message: String) {
        print!("{:<80}", message);
    }
}

impl UI for PrintUI {
    fn on_message(&mut self, message: UIMessage) {
        self.state.apply(message.clone());
        match message {
            UIMessage::Compilation { file, status } => {
                self.write_status(&status);
                self.write_message(format!("Compilation of {:?} ", file));
                self.write_status_details(&status);
            }
            UIMessage::CompilationStdout { file, content } => {
                println!("[STDOUT]  Compilation stdout of {:?}", file);
                print!("{}", content.trim());
            }
            UIMessage::CompilationStderr { file, content } => {
                println!("[STDERR]  Compilation stderr of {:?}", file);
                print!("{}", content.trim());
            }
            UIMessage::IOITask { task } => {
                cwrite!(self, BOLD, "Task {} ({})\n", task.title, task.name);
                println!("Path: {:?}", task.path);
                println!("Subtasks");
                for (st_num, subtask) in task.subtasks.iter().sorted_by_key(|x| x.0) {
                    println!("  {}: {} points", st_num, subtask.max_score);
                    print!("     testcases: [");
                    for tc_num in subtask.testcases.keys().sorted() {
                        print!(" {}", tc_num);
                    }
                    println!(" ]");
                }
            }
            UIMessage::IOIGeneration {
                subtask,
                testcase,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Generation of testcase {} of subtask {} ",
                    testcase, subtask
                ));
                self.write_status_details(&status);
            }
            UIMessage::IOIValidation {
                subtask,
                testcase,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Validation of testcase {} of subtask {} ",
                    testcase, subtask
                ));
                self.write_status_details(&status);
            }
            UIMessage::IOISolution {
                subtask,
                testcase,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Solution of testcase {} of subtask {} ",
                    testcase, subtask
                ));
                self.write_status_details(&status);
            }
            UIMessage::IOIEvaluation {
                subtask,
                testcase,
                solution,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Evaluation of {:?} of testcase {} of subtask {} ",
                    solution, testcase, subtask
                ));
                self.write_status_details(&status);
            }
            UIMessage::IOIChecker {
                subtask,
                testcase,
                solution,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Checking output of {:?} of testcase {} of subtask {} ",
                    solution, testcase, subtask
                ));
            }
            UIMessage::IOITestcaseScore {
                subtask,
                testcase,
                solution,
                score,
                message,
            } => {
                print!("[TESTCAS] ");
                self.write_message(format!(
                    "Solution {:?} scored {} on testcase {} of subtask {}: {}",
                    solution, score, testcase, subtask, message
                ));
            }
            UIMessage::IOISubtaskScore {
                subtask,
                solution,
                score,
            } => {
                print!("[SUBTASK] ");
                self.write_message(format!(
                    "Solution {:?} scored {} on subtask {} ",
                    solution, score, subtask
                ));
            }
            UIMessage::IOITaskScore { solution, score } => {
                print!("[TASK]    ");
                self.write_message(format!("Solution {:?} scored {} ", solution, score));
            }
        };
        println!();
    }

    fn finish(&mut self) {
        println!();
        println!();
        FinishUI::print(&self.state);
    }
}
