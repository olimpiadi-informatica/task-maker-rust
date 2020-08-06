use failure::Error;

use crate::sanity_checks::SanityCheck;
use crate::terry::TerryTask;
use crate::ui::UIMessage;
use crate::{EvaluationData, UISender};

/// Check that the validator is present.
#[derive(Debug, Default)]
pub struct ValidatorPresent;

impl SanityCheck<TerryTask> for ValidatorPresent {
    fn name(&self) -> &'static str {
        "ValidatorPresent"
    }

    fn pre_hook(&mut self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if task.validator.is_none() {
            eval.sender.send(UIMessage::Warning {
                message: "Validator not present".into(),
            })?;
        }
        Ok(())
    }
}
