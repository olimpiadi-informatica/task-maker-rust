use itertools::Itertools;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream};

use task_maker_dag::ExecutionStatus;

use crate::terry::CaseStatus;
use crate::ui::*;
use crate::{cwrite, ioi, terry};

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
    ioi_state: Option<ioi::ui_state::UIState>,
    terry_state: Option<terry::ui_state::UIState>,
}

impl PrintUI {
    /// Make a new PrintUI.
    pub fn new() -> PrintUI {
        PrintUI {
            stream: StandardStream::stdout(ColorChoice::Auto),
            ioi_state: None,
            terry_state: None,
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
    #[allow(clippy::cognitive_complexity)]
    fn on_message(&mut self, message: UIMessage) {
        if let Some(state) = self.ioi_state.as_mut() {
            state.apply(message.clone())
        }
        if let Some(state) = self.terry_state.as_mut() {
            state.apply(message.clone())
        }
        match message {
            UIMessage::StopUI => {}
            UIMessage::ServerStatus { status } => {
                println!(
                    "[STATUS]  Server status: {} ready exec, {} waiting exec",
                    status.ready_execs, status.waiting_execs
                );
                for worker in status.connected_workers {
                    if let Some(job) = &worker.current_job {
                        println!(" - {} ({}): {}", worker.name, worker.uuid, job.job);
                    } else {
                        println!(" - {} ({})", worker.name, worker.uuid);
                    }
                }
            }
            UIMessage::Compilation { file, status } => {
                self.write_status(&status);
                self.write_message(format!("Compilation of {:?} ", file));
                self.write_status_details(&status);
                if let UIExecutionStatus::Done { result } = status {
                    if let Some(stderr) = result.stderr {
                        let stderr = String::from_utf8_lossy(&stderr);
                        println!("\n[STDERR]  Compilation stderr of {:?}", file);
                        print!("{}", stderr.trim());
                    }
                    if let Some(stdout) = result.stdout {
                        let stdout = String::from_utf8_lossy(&stdout);
                        println!("\n[STDOUT]  Compilation stdout of {:?}", file);
                        print!("{}", stdout.trim());
                    }
                }
            }
            UIMessage::IOITask { task } => {
                self.ioi_state = Some(ioi::ui_state::UIState::new(task.as_ref()));

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
                if let UIExecutionStatus::Done { result } = status {
                    if let Some(stderr) = result.stderr {
                        let stderr = String::from_utf8_lossy(&stderr);
                        println!(
                            "\n[STDERR]  Generation stderr of testcase {} of subtask {}",
                            testcase, subtask
                        );
                        print!("{}", stderr.trim());
                    }
                }
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
                if let UIExecutionStatus::Done { result } = status {
                    if let Some(stderr) = result.stderr {
                        let stderr = String::from_utf8_lossy(&stderr);
                        println!(
                            "\n[STDERR]  Validation stderr of testcase {} of subtask {}",
                            testcase, subtask
                        );
                        print!("{}", stderr.trim());
                    }
                }
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
                part,
                num_parts,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Evaluation of {:?} of testcase {} of subtask {} (part {} of {}) ",
                    solution,
                    testcase,
                    subtask,
                    part + 1,
                    num_parts
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
                normalized_score,
            } => {
                print!("[SUBTASK] ");
                self.write_message(format!(
                    "Solution {:?} scored {} on subtask {} (normalized score {})",
                    solution, score, subtask, normalized_score,
                ));
            }
            UIMessage::IOITaskScore { solution, score } => {
                print!("[TASK]    ");
                self.write_message(format!("Solution {:?} scored {} ", solution, score));
            }
            UIMessage::IOIBooklet { name, status } => {
                self.write_status(&status);
                self.write_message(format!("Compilation of booklet {}", name));
            }
            UIMessage::IOIBookletDependency {
                booklet,
                name,
                step,
                num_steps,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Compilation of dependency {} of booklet {} (step {} of {})",
                    name,
                    booklet,
                    step + 1,
                    num_steps
                ));
            }
            UIMessage::Warning { message } => {
                cwrite!(self, WARNING, "[WARNING] ");
                print!("{}", message);
            }
            UIMessage::TerryTask { task } => {
                self.terry_state = Some(terry::ui_state::UIState::new(task.as_ref()));
            }
            UIMessage::TerryGeneration {
                solution,
                seed,
                status,
            } => {
                self.write_status(&status);
                self.write_message(format!(
                    "Generation of input for {} with seed {} ",
                    solution.display(),
                    seed
                ));
                self.write_status_details(&status);
            }
            UIMessage::TerryValidation { solution, status } => {
                self.write_status(&status);
                self.write_message(format!("Validation of input for {} ", solution.display()));
                self.write_status_details(&status);
            }
            UIMessage::TerrySolution { solution, status } => {
                self.write_status(&status);
                self.write_message(format!("Solving input for {} ", solution.display()));
                self.write_status_details(&status);
            }
            UIMessage::TerryChecker { solution, status } => {
                self.write_status(&status);
                self.write_message(format!("Checking output of {} ", solution.display()));
                self.write_status_details(&status);
            }
            UIMessage::TerrySolutionOutcome { solution, outcome } => match outcome {
                Ok(outcome) => {
                    cwrite!(self, SUCCESS, "[OUTCOME] ");
                    println!("Solution {} scored {}", solution.display(), outcome.score);
                    print!("Validation: ");
                    for case in outcome.validation.cases.iter() {
                        match case.status {
                            CaseStatus::Missing => cwrite!(self, WARNING, "m "),
                            CaseStatus::Parsed => cwrite!(self, SUCCESS, "p "),
                            CaseStatus::Invalid => cwrite!(self, ERROR, "i "),
                        }
                    }
                    println!();
                    for (i, case) in outcome.validation.cases.iter().enumerate() {
                        if let Some(message) = &case.message {
                            println!("    Case {}: {}", i + 1, message);
                        }
                    }
                    for alert in outcome.validation.alerts.iter() {
                        println!("    [{}] {}", alert.severity, alert.message);
                    }
                    print!("Feedback:   ");
                    for case in outcome.feedback.cases.iter() {
                        if case.correct {
                            cwrite!(self, SUCCESS, "c ");
                        } else {
                            cwrite!(self, ERROR, "w ");
                        }
                    }
                    println!();
                    for (i, case) in outcome.feedback.cases.iter().enumerate() {
                        if let Some(message) = &case.message {
                            println!("    Case {}: {}", i + 1, message);
                        }
                    }
                    for alert in outcome.feedback.alerts.iter() {
                        println!("    [{}] {}", alert.severity, alert.message);
                    }
                }
                Err(e) => {
                    cwrite!(self, ERROR, "[OUTCOME] ");
                    print!("Checker of {} failed: {}", solution.display(), e);
                }
            },
        };
        println!();
    }

    fn finish(&mut self) {
        println!();
        println!();
        if let Some(state) = self.ioi_state.as_ref() {
            ioi::finish_ui::FinishUI::print(state);
        }
        if let Some(state) = self.terry_state.as_ref() {
            terry::finish_ui::FinishUI::print(state);
        }
    }
}

impl Default for PrintUI {
    fn default() -> Self {
        Self::new()
    }
}
