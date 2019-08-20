use crate::ioi::ui_state::{CompilationStatus, UIState, SolutionEvaluationState};
use failure::Error;
use itertools::Itertools;
use std::io::Write;
use std::path::Path;

use termcolor::{ColorChoice, StandardStream};

type Result = std::result::Result<(), Error>;

/// Print the final state of the execution of a task.
pub fn print_final_state(state: &UIState) {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    print_task_info(&mut stdout, state).unwrap();
    print_compilations(&mut stdout, state).unwrap();
    print_generations(&mut stdout, state).unwrap();
    print_evaluations(&mut stdout, state).unwrap();
}

/// Print the basic task information.
fn print_task_info(stdout: &mut StandardStream, state: &UIState) -> Result {
    writeln!(stdout, "{} ({})", state.task.title, state.task.name)?;
    writeln!(stdout, "Path:      {}", state.task.path.to_string_lossy())?;
    writeln!(stdout, "Max score: {}", state.max_score)?;
    writeln!(stdout)?;
    Ok(())
}

/// Print the compilation info.
fn print_compilations(stdout: &mut StandardStream, state: &UIState) -> Result {
    writeln!(stdout, "Compilations")?;
    for (path, status) in &state.compilations {
        writeln!(
            stdout,
            "{:20} {}",
            path.file_name().unwrap().to_string_lossy(),
            match status {
                CompilationStatus::Done { result } => format!(
                    "Done | {:.3}s | {:.1}MiB",
                    result.resources.cpu_time,
                    (result.resources.memory as f64) / 1024.0
                ),
                CompilationStatus::Failed { result } => format!(
                    "Fail | {:.3}s | {:.1}MiB",
                    result.resources.cpu_time,
                    (result.resources.memory as f64) / 1024.0
                ),
                _ => format!("{:?}", status),
            }
        )?;
    }
    writeln!(stdout)?;
    Ok(())
}

/// Print the generation info.
fn print_generations(stdout: &mut StandardStream, state: &UIState) -> Result {
    writeln!(stdout, "Generations")?;
    for (st_num, subtask) in state.generations.iter().sorted_by_key(|(n, _)| *n) {
        writeln!(stdout, "Subtask {}", st_num)?;
        for (tc_num, testcase) in subtask.testcases.iter().sorted_by_key(|(n, _)| *n) {
            let mut state = vec![];
            if testcase.generation.is_some() {
                state.push("Generated");
            }
            if testcase.validation.is_some() {
                state.push("Validated");
            }
            if testcase.solution.is_some() {
                state.push("Solved");
            }
            writeln!(stdout, "  Testcase {:<3}    {}", tc_num, state.join(" | "))?;
        }
    }
    writeln!(stdout)?;
    Ok(())
}

/// Print the evaluation info of all the solutions.
fn print_evaluations(stdout: &mut StandardStream, state: &UIState) -> Result {
    writeln!(stdout, "Evaluations")?;
    for (path, eval) in &state.evaluations {
        print_evaluation(stdout, path, &eval.score, state.max_score, eval, state)?;
    }
    Ok(())
}

/// Print the evaluation info of a single solution.
fn print_evaluation(
    stdout: &mut StandardStream,
    path: &Path,
    score: &Option<f64>,
    max_score: f64,
    eval: &SolutionEvaluationState,
    state: &UIState,
) -> Result {
    writeln!(stdout)?;
    writeln!(
        stdout,
        "{}: {:.2} / {:.2}",
        path.file_name().unwrap().to_string_lossy(),
        score.unwrap_or(0.0),
        max_score
    )?;
    for (st_num, subtask) in eval.subtasks.iter().sorted_by_key(|(n, _)| *n) {
        writeln!(
            stdout,
            "Subtask #{}: {:.2} / {:.2}",
            st_num,
            subtask.score.unwrap_or(0.0),
            state.task.subtasks.get(st_num).unwrap().max_score
        )?;
        for (tc_num, testcase) in subtask.testcases.iter().sorted_by_key(|(n, _)| *n) {
            writeln!(
                stdout,
                "{:>3}) [{:.2}] [{:.3}s | {:.1}MiB] {}",
                tc_num,
                testcase.score.unwrap_or(0.0),
                testcase
                    .result
                    .as_ref()
                    .map(|r| r.resources.cpu_time)
                    .unwrap_or(0.0),
                (testcase
                    .result
                    .as_ref()
                    .map(|r| r.resources.memory)
                    .unwrap_or(0) as f64)
                    / 1024.0,
                format!("{:?}", testcase.status)
            )?;
        }
    }

    Ok(())
}
