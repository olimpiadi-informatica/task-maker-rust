use anyhow::Error;
use itertools::Itertools;

use crate::sanity_checks::SanityCheck;
use crate::{EvaluationData, IOITask, UISender};

/// Check that all the subtasks have a name.
#[derive(Debug, Default)]
pub struct MissingSubtaskNames;

impl SanityCheck<IOITask> for MissingSubtaskNames {
    fn name(&self) -> &'static str {
        "MissingSubtaskNames"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut missing_name = vec![];
        for subtask_id in task.subtasks.keys().sorted() {
            let subtask = &task.subtasks[subtask_id];
            if subtask.name.is_none() {
                missing_name.push(format!(
                    "Subtask {} ({} points)",
                    subtask.id, subtask.max_score
                ));
            }
        }
        if !missing_name.is_empty() {
            eval.sender.send_warning(format!(
                "These subtasks are missing a name: {}",
                missing_name.join(", ")
            ))?;
        }
        Ok(())
    }
}
