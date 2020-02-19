use failure::Error;

use crate::sanity_checks::SanityCheck;
use crate::terry::Task;
use crate::ui::UIMessage;
use crate::{EvaluationData, UISender};

/// Check that the validator is present.
#[derive(Debug, Default)]
pub struct ValidatorPresent;

impl SanityCheck<Task> for ValidatorPresent {
    fn name(&self) -> &'static str {
        "ValidatorPresent"
    }

    fn pre_hook(&mut self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        if task.validator.is_none() {
            eval.sender.send(UIMessage::Warning {
                message: "Validator not present".into(),
            })?;
        }
        Ok(())
    }
}
