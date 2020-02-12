use failure::Error;

use crate::ioi::Task;
use crate::ui::{UIMessage, UIMessageSender};
use crate::{list_files, EvaluationData, UISender};

/// The default maximum score of a task.
const DEFAULT_TASK_MAX_SCORE: f64 = 100.0;

/// Check that the task has the usual maximum score.
pub fn check_task_max_score(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let task_score: f64 = task.subtasks.values().map(|st| st.max_score).sum();
    if approx::abs_diff_ne!(task_score, DEFAULT_TASK_MAX_SCORE) {
        eval.sender.send(UIMessage::Warning {
            message: format!(
                "The score of the task is {} (not {})",
                task_score, DEFAULT_TASK_MAX_SCORE
            ),
        })?;
    }
    Ok(())
}

/// Check that there are no broken links.
pub fn check_broken_symlinks(task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
    for file in list_files(&task.path, vec!["**/*"]) {
        if !file.exists() && file.read_link().is_ok() {
            ui.send(UIMessage::Warning {
                message: format!(
                    "{} is a broken link",
                    file.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    Ok(())
}
