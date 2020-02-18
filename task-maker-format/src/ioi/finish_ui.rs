use std::path::Path;

use itertools::Itertools;
use termcolor::{ColorChoice, ColorSpec, StandardStream};

use task_maker_dag::ExecutionStatus;

use crate::ioi::ui_state::{SolutionEvaluationState, TestcaseEvaluationStatus, UIState};
use crate::ui::{
    FinishUI as FinishUITrait, FinishUIUtils, UIExecutionStatus, BLUE, BOLD, GREEN, RED, YELLOW,
};
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
        ui.print_booklets(state);
        println!();
        ui.print_generations(state);
        println!();
        ui.print_evaluations(state);
        ui.print_summary(state);
        ui.print_messages(state);
    }
}

impl FinishUI {
    /// Print the basic task info.
    fn print_task_info(&mut self, state: &UIState) {
        cwrite!(self, BOLD, "Task:         ");
        println!("{} ({})", state.task.title, state.task.name);
        cwrite!(self, BOLD, "Path:         ");
        println!("{}", state.task.path.display());
        cwrite!(self, BOLD, "Max score:    ");
        println!("{}", state.max_score);
        cwrite!(self, BOLD, "Time limit:   ");
        println!(
            "{}",
            state
                .task
                .time_limit
                .map(|t| format!("{}s", t))
                .unwrap_or_else(|| "unlimited".to_string())
        );
        cwrite!(self, BOLD, "Memory limit: ");
        println!(
            "{}",
            state
                .task
                .memory_limit
                .map(|t| format!("{}MiB", t))
                .unwrap_or_else(|| "unlimited".to_string())
        );
    }

    /// Print all the booklet states.
    fn print_booklets(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Statements");
        for name in state.booklets.keys().sorted() {
            let booklet = &state.booklets[name];
            cwrite!(self, BOLD, "{:<20}  ", name);
            self.print_execution_status(&booklet.status);
            println!();
            for name in booklet.dependencies.keys().sorted() {
                let dep = &booklet.dependencies[name];
                print!("  {:<18}  ", name);
                let mut first = true;
                for step in dep.iter() {
                    if first {
                        first = false;
                    } else {
                        print!(" | ");
                    }
                    self.print_execution_status(&step.status);
                }
                println!();
            }
        }
    }

    /// Print all the generation states.
    fn print_generations(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Generations");
        for (st_num, subtask) in state.generations.iter().sorted_by_key(|(n, _)| *n) {
            cwrite!(self, BOLD, "Subtask {}", st_num);
            println!(": {} points", state.task.subtasks[&st_num].max_score);
            for (tc_num, testcase) in subtask.testcases.iter().sorted_by_key(|(n, _)| *n) {
                print!("#{:<3} ", tc_num);

                let mut first = true;
                let mut gen_failed = false;
                let mut val_failed = false;
                if let Some(gen) = &testcase.generation {
                    if let ExecutionStatus::Success = gen.status {
                        cwrite!(self, GREEN, "Generated");
                    } else {
                        cwrite!(self, YELLOW, "Generation failed: {:?}", gen.status);
                        gen_failed = true;
                    }
                    first = false;
                }
                if let Some(val) = &testcase.validation {
                    if !first {
                        print!(" | ");
                    }
                    if let ExecutionStatus::Success = val.status {
                        cwrite!(self, GREEN, "Validated");
                    } else {
                        cwrite!(self, YELLOW, "Validation failed: {:?}", val.status);
                        val_failed = true;
                    }
                    first = false;
                }
                if let Some(sol) = &testcase.solution {
                    if !first {
                        print!(" | ");
                    }
                    if let ExecutionStatus::Success = sol.status {
                        cwrite!(self, GREEN, "Solved");
                    } else {
                        cwrite!(self, YELLOW, "Solution failed: {:?}", sol.status);
                    }
                }
                println!();
                if gen_failed {
                    if let Some(stderr) = &testcase.generation_stderr {
                        if !stderr.trim().is_empty() {
                            cwriteln!(self, BOLD, "Generation stderr:");
                            println!("{}", stderr.trim());
                        }
                    }
                }
                if val_failed {
                    if let Some(stderr) = &testcase.validation_stderr {
                        if !stderr.trim().is_empty() {
                            cwriteln!(self, BOLD, "Validation stderr:");
                            println!("{}", stderr.trim());
                        }
                    }
                }
            }
        }
    }

    /// Print all the evaluation states.
    fn print_evaluations(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Evaluations");
        for path in state.evaluations.keys().sorted() {
            let eval = &state.evaluations[path];
            self.print_evaluation(
                path,
                eval.score.unwrap_or(0.0),
                state.max_score,
                eval,
                state,
            );
            println!();
        }
    }

    /// Print the state of the evaluation of a single solution.
    fn print_evaluation(
        &mut self,
        path: &Path,
        score: f64,
        max_score: f64,
        eval: &SolutionEvaluationState,
        state: &UIState,
    ) {
        let name = path
            .file_name()
            .expect("Invalid file name")
            .to_string_lossy();
        cwrite!(self, BOLD, "{}", name);
        print!(": ");
        self.print_score_frac(score, max_score);
        println!();
        for (st_num, subtask) in eval.subtasks.iter().sorted_by_key(|(n, _)| *n) {
            cwrite!(self, BOLD, "Subtask #{}", st_num);
            print!(": ");
            let max_score = state.task.subtasks[&st_num].max_score;
            let score = subtask.score.unwrap_or(0.0);
            self.print_score_frac(score, max_score);
            println!();
            for (tc_num, testcase) in subtask.testcases.iter().sorted_by_key(|(n, _)| *n) {
                print!("{:3}) ", tc_num);
                let score = testcase.score.unwrap_or(0.0);
                if abs_diff_eq!(score, 1.0) {
                    cwrite!(self, GREEN, "[{:.2}]", score);
                } else if abs_diff_eq!(score, 0.0) {
                    cwrite!(self, RED, "[{:.2}]", score);
                } else {
                    cwrite!(self, YELLOW, "[{:.2}]", score);
                }
                if let Some(result) = &testcase.result {
                    print!(" [");
                    FinishUIUtils::print_time_memory(&result.resources);
                    print!("]");
                }
                print!(" {}", testcase.status.message());
                if let Some(result) = &testcase.result {
                    match &result.status {
                        ExecutionStatus::ReturnCode(code) => print!(": Exited with {}", code),
                        ExecutionStatus::Signal(sig, name) => print!(": Signal {} ({})", sig, name),
                        ExecutionStatus::InternalError(err) => print!(": Internal error: {}", err),
                        _ => {}
                    }
                    if result.was_killed {
                        print!(" (killed)");
                    }
                    if result.was_cached {
                        print!(" (from cache)");
                    }
                }
                if FinishUI::is_ansi() {
                    self.print_right(format!("[{}]", name));
                }
                println!();
            }
        }
    }

    fn print_summary(&mut self, state: &UIState) {
        cwriteln!(self, BLUE, "Summary");
        let max_len = FinishUIUtils::get_max_len(&state.evaluations);
        print!("{:width$} ", "", width = max_len);
        cwrite!(self, BOLD, "{:^5}| ", state.max_score);
        for st_num in state.task.subtasks.keys().sorted() {
            let subtask = &state.task.subtasks[st_num];
            cwrite!(self, BOLD, " {:^3.0} ", subtask.max_score);
        }
        println!();
        for path in state.evaluations.keys().sorted() {
            let eval = &state.evaluations[path];
            print!(
                "{:>width$} ",
                path.file_name()
                    .expect("Invalid file name")
                    .to_string_lossy(),
                width = max_len
            );
            print!("{:^5.0}| ", eval.score.unwrap_or(0.0));
            for st_num in eval.subtasks.keys().sorted() {
                let subtask = &eval.subtasks[&st_num];
                let score = subtask.score.unwrap_or(0.0);
                let normalized_score = subtask.normalized_score.unwrap_or(0.0);
                let color = self.score_color(normalized_score);
                cwrite!(self, color, " {:^3.0} ", score);
            }
            print!("  ");
            for st_num in eval.subtasks.keys().sorted() {
                let subtask = &eval.subtasks[&st_num];
                let normalized_score = subtask.normalized_score.unwrap_or(0.0);
                let color = self.score_color(normalized_score);
                cwrite!(self, color, "[");
                for tc_num in subtask.testcases.keys().sorted() {
                    let testcase = &subtask.testcases[tc_num];
                    use TestcaseEvaluationStatus::*;
                    match testcase.status {
                        Accepted(_) => cwrite!(self, GREEN, "A"),
                        WrongAnswer(_) => cwrite!(self, RED, "W"),
                        Partial(_) => cwrite!(self, YELLOW, "P"),
                        TimeLimitExceeded => cwrite!(self, RED, "T"),
                        WallTimeLimitExceeded => cwrite!(self, RED, "T"),
                        MemoryLimitExceeded => cwrite!(self, RED, "M"),
                        RuntimeError => cwrite!(self, RED, "R"),
                        Failed => cwrite!(self, BOLD, "F"),
                        Skipped => cwrite!(self, BOLD, "S"),
                        _ => cwrite!(self, BOLD, "X"),
                    }
                }
                cwrite!(self, color, "]");
            }
            println!();
        }
        println!();
    }

    /// Print the warnings.
    fn print_messages(&mut self, state: &UIState) {
        if !state.warnings.is_empty() {
            cwriteln!(self, YELLOW, "Warnings:");
            for warning in state.warnings.iter() {
                println!(" - {}", warning);
            }
        }
    }

    /// Print the score fraction of a solution using colors.
    fn print_score_frac(&mut self, score: f64, max_score: f64) {
        if max_score == 0.0 {
            print!("{:.2} / {:.2}", score, max_score);
        } else {
            let color = self.score_color(score / max_score);
            cwrite!(self, color, "{:.2} / {:.2}", score, max_score);
        }
    }

    fn score_color(&mut self, normalized_score: f64) -> &'static ColorSpec {
        if abs_diff_eq!(normalized_score, 1.0) {
            &GREEN
        } else if abs_diff_eq!(normalized_score, 0.0) {
            &RED
        } else {
            &YELLOW
        }
    }

    /// Print some text to the right of the screen. Note that this will print some ANSI escape
    /// sequences.
    fn print_right(&mut self, what: String) {
        // \x1b[1000C  move the cursor to the right margin
        // \x1b[{}D    move the cursor left by {} characters
        print!("\x1b[1000C\x1b[{}D{}", what.len() - 1, what);
    }

    /// Check if ANSI is supported: if not in windows and not in a "dumb" terminal.
    fn is_ansi() -> bool {
        !cfg!(windows) && std::env::var("TERM").map(|v| v != "dumb").unwrap_or(false)
    }

    /// Print the status of an `UIExecutionStatus` using colors.
    fn print_execution_status(&mut self, status: &UIExecutionStatus) {
        match status {
            UIExecutionStatus::Pending => print!("..."),
            UIExecutionStatus::Skipped => print!("skipped"),
            UIExecutionStatus::Started { .. } => cwrite!(self, YELLOW, "started"),
            UIExecutionStatus::Done { result } => match &result.status {
                ExecutionStatus::Success => cwrite!(self, GREEN, "Success"),
                _ => cwrite!(self, RED, "{:?}", result.status),
            },
        }
    }
}
