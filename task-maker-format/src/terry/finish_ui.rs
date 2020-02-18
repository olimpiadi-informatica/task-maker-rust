use std::path::Path;

use itertools::Itertools;
use termcolor::{ColorChoice, StandardStream};

use task_maker_dag::{ExecutionResult, ExecutionStatus};

use crate::terry::ui_state::{SolutionStatus, UIState};
use crate::terry::CaseStatus;
use crate::ui::{FinishUI as FinishUITrait, FinishUIUtils, BLUE, BOLD, GREEN, RED, YELLOW};
use crate::{cwrite, cwriteln};

/// UI that prints to `stdout` the ending result of the evaluation of a IOI task.
pub struct FinishUI {
    /// Stream where to print to.
    stream: StandardStream,
}

impl FinishUITrait<UIState> for FinishUI {
    fn print(state: &UIState) {
        let mut ui = FinishUI {
            stream: StandardStream::stdout(ColorChoice::Auto),
        };

        ui.print_task_info(state);
        println!();
        FinishUIUtils::new(&mut ui.stream).print_compilations(&state.compilations);
        println!();
        ui.print_evaluations(state);
        ui.print_summary(state);
    }
}

impl FinishUI {
    fn print_task_info(&mut self, state: &UIState) {
        cwrite!(self, BOLD, "Task:      ");
        println!("{} ({})", state.task.description, state.task.name);
        cwrite!(self, BOLD, "Path:      ");
        println!("{}", state.task.path.display());
        cwrite!(self, BOLD, "Max score: ");
        println!("{}", state.task.max_score);
    }

    fn print_evaluations(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Evaluations");
        for (path, solution) in state.solutions.iter().sorted_by_key(|(n, _)| *n) {
            let name = path.file_name().expect("Invalid file name");
            cwrite!(self, BOLD, "{} ", Path::new(name).display());
            match &solution.outcome {
                Some(Ok(outcome)) => {
                    let score = outcome.score * state.task.max_score;
                    if abs_diff_eq!(outcome.score, 0.0) {
                        cwriteln!(self, RED, "{:.2} / {:.2}", score, state.task.max_score);
                    } else if abs_diff_eq!(outcome.score, 1.0) {
                        cwriteln!(self, GREEN, "{:.2} / {:.2}", score, state.task.max_score);
                    } else {
                        cwriteln!(self, YELLOW, "{:.2} / {:.2}", score, state.task.max_score);
                    }
                }
                Some(Err(e)) => {
                    println!();
                    cwrite!(self, RED, "Fail: ");
                    println!("{}", e);
                }
                None => {
                    println!();
                }
            }
            if let Some(seed) = solution.seed {
                println!("      Seed: {}", seed);
            }

            let print_result = |result: &Option<ExecutionResult>| {
                if let Some(result) = &result {
                    FinishUIUtils::print_time_memory(&result.resources);
                    if let ExecutionStatus::Success = result.status {
                    } else {
                        print!(" | ");
                        FinishUIUtils::print_fail_execution_status(&result.status);
                    }
                    if result.was_cached {
                        print!(" (cached)");
                    }
                    if result.was_killed {
                        print!(" (killed)");
                    }
                } else {
                    print!("unknown");
                }
            };

            print!("Generation: ");
            print_result(&solution.generator_result);
            println!();

            print!("Validation: ");
            print_result(&solution.validator_result);
            println!();

            print!("Evaluation: ");
            print_result(&solution.solution_result);
            println!();

            print!("   Checker: ");
            print_result(&solution.checker_result);
            println!();
            println!();
        }
    }

    /// Print the summary of the solution results.
    fn print_summary(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Summary");
        let max_len = FinishUIUtils::get_max_len(&state.solutions);
        for (path, solution) in state.solutions.iter().sorted_by_key(|(n, _)| *n) {
            print!(
                "{:>width$} ",
                Path::new(path.file_name().expect("Invalid file name")).display(),
                width = max_len
            );
            match &solution.outcome {
                Some(Ok(outcome)) => {
                    let score = outcome.score * state.task.max_score;
                    if abs_diff_eq!(outcome.score, 1.0) {
                        cwrite!(self, GREEN, "{:>3}", score.floor());
                    } else if abs_diff_eq!(outcome.score, 0.0) {
                        cwrite!(self, RED, "{:>3}", score.floor());
                    } else {
                        cwrite!(self, YELLOW, "{:>3}", score.floor());
                    }
                    for (val, feed) in outcome
                        .validation
                        .cases
                        .iter()
                        .zip(outcome.feedback.cases.iter())
                    {
                        match val.status {
                            CaseStatus::Missing => print!(" m"),
                            CaseStatus::Parsed => {
                                if feed.correct {
                                    cwrite!(self, GREEN, " c");
                                } else {
                                    cwrite!(self, RED, " w");
                                }
                            }
                            CaseStatus::Invalid => cwrite!(self, RED, " i"),
                        }
                    }
                }
                Some(Err(e)) => {
                    print!("    {}", e);
                }
                None => {
                    if let SolutionStatus::Failed(e) = &solution.status {
                        print!("    {}", e);
                    } else {
                        print!("    Failed");
                    }
                }
            }
            println!();
        }
    }
}
