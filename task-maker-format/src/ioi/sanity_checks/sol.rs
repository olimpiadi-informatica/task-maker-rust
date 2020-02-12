use failure::Error;

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::Task;
use crate::ui::UIMessage;
use crate::{list_files, EvaluationData, UISender};

/// Check that all the graders inside sol are present.
pub fn check_sol_graders(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    check_missing_graders(task, eval, "sol")
}

/// Check that the official solution is a symlink.
pub fn check_sol_symlink(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    for solution in list_files(&task.path, vec!["sol/solution.*", "sol/soluzione.*"]) {
        if solution.read_link().is_err() {
            eval.sender.send(UIMessage::Warning {
                message: format!(
                    "Solution {} is not a symlink",
                    solution.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    Ok(())
}

/// Check that the official solution is unique.
pub fn check_sol_unique(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let solutions: Vec<_> = list_files(&task.path, vec!["sol/solution.*", "sol/soluzione.*"])
        .into_iter()
        .map(|s| s.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    if solutions.len() > 1 {
        eval.sender.send(UIMessage::Warning {
            message: format!("More than an official solution found: {:?}", solutions),
        })?;
    }
    Ok(())
}
