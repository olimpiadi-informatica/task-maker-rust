use failure::Error;

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::IOITask;
use crate::sanity_checks::SanityCheck;
use crate::ui::UIMessage;
use crate::{list_files, EvaluationData, UISender};

/// Check that all the graders inside sol are present.
#[derive(Debug, Default)]
pub struct SolGraders;

impl SanityCheck<IOITask> for SolGraders {
    fn name(&self) -> &'static str {
        "SolGraders"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        check_missing_graders(task, eval, "sol")
    }
}

/// Check that the official solution is a symlink.
#[derive(Debug, Default)]
pub struct SolSymlink;

impl SanityCheck<IOITask> for SolSymlink {
    fn name(&self) -> &'static str {
        "SolSymlink"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
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
}
