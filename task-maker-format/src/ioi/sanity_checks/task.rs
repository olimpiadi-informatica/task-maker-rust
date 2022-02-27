use anyhow::Error;

use crate::ioi::IOITask;
use crate::sanity_checks::SanityCheck;
use crate::ui::UIMessageSender;
use crate::{list_files, EvaluationData, UISender};

/// The default maximum score of a task.
const DEFAULT_TASK_MAX_SCORE: f64 = 100.0;

/// Check that the task has the usual maximum score.
#[derive(Debug, Default)]
pub struct TaskMaxScore;

impl SanityCheck<IOITask> for TaskMaxScore {
    fn name(&self) -> &'static str {
        "TaskMaxScore"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let task_score: f64 = task.subtasks.values().map(|st| st.max_score).sum();
        if approx::abs_diff_ne!(task_score, DEFAULT_TASK_MAX_SCORE) {
            eval.sender.send_error(format!(
                "The score of the task is {} (not {})",
                task_score, DEFAULT_TASK_MAX_SCORE
            ))?;
        }
        Ok(())
    }
}

/// Check that there are no broken links.
#[derive(Debug, Default)]
pub struct BrokenSymlinks;

impl SanityCheck<IOITask> for BrokenSymlinks {
    fn name(&self) -> &'static str {
        "BrokenSymlinks"
    }

    fn post_hook(&mut self, task: &IOITask, ui: &mut UIMessageSender) -> Result<(), Error> {
        for file in list_files(&task.path, vec!["**/*"]) {
            if !file.exists() && file.read_link().is_ok() {
                ui.send_warning(format!(
                    "{} is a broken link",
                    file.strip_prefix(&task.path).unwrap().display()
                ))?;
            }
        }
        Ok(())
    }
}
