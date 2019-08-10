use crate::task_types::ioi::{IOISubtaskId, IOITestcaseId};
use crate::ui::ioi_state::IOIUIState;
use crate::ui::ioi_state::TestcaseEvaluationStatus;
use failure::Error;
use itertools::Itertools;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use termcolor::{ColorChoice, StandardStream};

type Result = std::result::Result<(), Error>;

/// Print the final state of the execution of a task.
pub fn print_final_state(state: &IOIUIState) {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    print_task_info(&mut stdout, state).unwrap();
    print_compilations(&mut stdout, state).unwrap();
    print_generations(&mut stdout, state).unwrap();
    print_evaluations(&mut stdout, state).unwrap();
}

/// Print the basic task information.
fn print_task_info(stdout: &mut StandardStream, state: &IOIUIState) -> Result {
    writeln!(stdout, "{} ({})", state.title, state.name)?;
    writeln!(stdout, "Path:      {}", state.path.to_string_lossy())?;
    writeln!(stdout, "Max score: {}", state.max_score)?;
    writeln!(stdout)?;
    Ok(())
}

/// Print the compilation info.
fn print_compilations(stdout: &mut StandardStream, state: &IOIUIState) -> Result {
    writeln!(stdout, "Compilations")?;
    for (path, status) in &state.compilations {
        writeln!(
            stdout,
            "{:20} {:?}",
            path.file_name().unwrap().to_string_lossy(),
            status
        )?;
    }
    writeln!(stdout)?;
    Ok(())
}

/// Print the generation info.
fn print_generations(stdout: &mut StandardStream, state: &IOIUIState) -> Result {
    writeln!(stdout, "Generations")?;
    for (st_num, subtask) in state.generations.iter().sorted_by_key(|(n, _)| *n) {
        writeln!(stdout, "Subtask {}", st_num)?;
        for (tc_num, testcase) in subtask.iter().sorted_by_key(|(n, _)| *n) {
            writeln!(stdout, "  Testcase {:3} {:?}", tc_num, testcase)?;
        }
    }
    writeln!(stdout)?;
    Ok(())
}

/// Print the evaluation info of all the solutions.
fn print_evaluations(stdout: &mut StandardStream, state: &IOIUIState) -> Result {
    writeln!(stdout, "Evaluations")?;
    for (path, eval) in &state.evaluations {
        let score = state.solution_scores.get(path).unwrap();
        print_evaluation(stdout, path, score, state.max_score, eval)?;
    }
    Ok(())
}

/// Print the evaluation info of a single solution.
fn print_evaluation(
    stdout: &mut StandardStream,
    path: &Path,
    score: &Option<f64>,
    max_score: f64,
    eval: &HashMap<IOISubtaskId, HashMap<IOITestcaseId, TestcaseEvaluationStatus>>,
) -> Result {
    writeln!(stdout)?;
    writeln!(
        stdout,
        "{}: {:.2} / {:.2}",
        path.file_name().unwrap().to_string_lossy(),
        score.unwrap_or(0.0),
        max_score
    )?;
    for (st_num, subtask) in eval.iter().sorted_by_key(|(n, _)| *n) {
        writeln!(stdout, "Subtask #{}: ?? / ??", st_num)?;
        for (tc_num, testcase) in subtask.iter().sorted_by_key(|(n, _)| *n) {
            writeln!(stdout, "{:>3}) {:?}", tc_num, testcase)?;
        }
    }

    Ok(())
}
