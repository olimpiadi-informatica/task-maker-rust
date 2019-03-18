use crate::execution::ExecutionStatus;
use crate::ui::*;
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// A simple UI that will print to stdout the human readable messages. Useful
/// for debugging or for when curses is not available.
pub struct PrintUI {
    stdout: StandardStream,
    error_style: ColorSpec,
    warning_style: ColorSpec,
    info_style: ColorSpec,
    success_style: ColorSpec,
}

impl PrintUI {
    /// Make a new PrintUI.
    pub fn new() -> PrintUI {
        let mut ui = PrintUI {
            stdout: StandardStream::stdout(ColorChoice::Auto),
            error_style: ColorSpec::new(),
            warning_style: ColorSpec::new(),
            info_style: ColorSpec::new(),
            success_style: ColorSpec::new(),
        };
        ui.error_style.set_fg(Some(Color::Red)).set_bold(true);
        ui.warning_style
            .set_fg(Some(Color::Yellow))
            .set_intense(true)
            .set_bold(true);
        ui.info_style
            .set_fg(Some(Color::Blue))
            .set_intense(true)
            .set_bold(true);
        ui.success_style
            .set_fg(Some(Color::Green))
            .set_intense(true)
            .set_bold(true);
        ui
    }

    fn write_status(&mut self, status: &UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => write!(&mut self.stdout, "[PENDING] ").unwrap(),
            UIExecutionStatus::Started { .. } => write!(&mut self.stdout, "[STARTED] ").unwrap(),
            UIExecutionStatus::Done { result } => match result.result.status {
                ExecutionStatus::Success => self.write_info("[DONE]    ".to_owned()),
                _ => self.write_warning("[DONE]    ".to_owned()),
            },
            UIExecutionStatus::Skipped => self.write_warning("[SKIPPED] ".to_owned()),
        };
        self.stdout.reset().unwrap();
    }

    fn write_status_details(&mut self, status: &UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => {}
            UIExecutionStatus::Started { worker } => {
                write!(&mut self.stdout, "Worker: {:?}", worker).unwrap();
            }
            UIExecutionStatus::Done { result } => {
                self.write_execution_status(&result.result.status);
            }
            UIExecutionStatus::Skipped => {}
        }
    }

    fn write_execution_status(&mut self, status: &ExecutionStatus) {
        match status {
            ExecutionStatus::Success => self.write_success(format!("[{:?}]", status)),
            ExecutionStatus::InternalError(_) => self.write_error(format!("[{:?}]", status)),
            _ => self.write_warning(format!("[{:?}]", status)),
        }
    }

    fn write_error(&mut self, message: String) {
        self.stdout.set_color(&self.error_style).unwrap();
        write!(&mut self.stdout, "{}", message).unwrap();
        self.stdout.reset().unwrap();
    }

    fn write_warning(&mut self, message: String) {
        self.stdout.set_color(&self.warning_style).unwrap();
        write!(&mut self.stdout, "{}", message).unwrap();
        self.stdout.reset().unwrap();
    }

    fn write_info(&mut self, message: String) {
        self.stdout.set_color(&self.info_style).unwrap();
        write!(&mut self.stdout, "{}", message).unwrap();
        self.stdout.reset().unwrap();
    }

    fn write_success(&mut self, message: String) {
        self.stdout.set_color(&self.success_style).unwrap();
        write!(&mut self.stdout, "{}", message).unwrap();
        self.stdout.reset().unwrap();
    }

    fn write_message(&mut self, message: String) {
        write!(&mut self.stdout, "{:<80}", message).unwrap();
    }
}

impl UI for PrintUI {
    fn on_message(&mut self, message: UIMessage) {
        match message {
            UIMessage::Compilation { file, status } => {
                self.write_status(&status);
                self.write_message(format!("Compilation of {:?} ", file));
                self.write_status_details(&status);
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
            } => {
                write!(&mut self.stdout, "[TESTCAS] ").unwrap();
                self.write_message(format!(
                    "Solution {:?} scored {} on testcase {} of subtask {} ",
                    solution, score, testcase, subtask
                ));
            }
            UIMessage::IOISubtaskScore {
                subtask,
                solution,
                score,
            } => {
                write!(&mut self.stdout, "[SUBTASK] ").unwrap();
                self.write_message(format!(
                    "Solution {:?} scored {} on subtask {} ",
                    solution, score, subtask
                ));
            }
            UIMessage::IOITaskScore { solution, score } => {
                write!(&mut self.stdout, "[TASK]    ").unwrap();
                self.write_message(format!("Solution {:?} scored {} ", solution, score));
            }
        };
        write!(&mut self.stdout, "\n").unwrap();
    }
}
