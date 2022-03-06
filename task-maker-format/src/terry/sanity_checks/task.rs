use anyhow::Error;
use task_maker_diagnostics::Diagnostic;

use crate::sanity_checks::SanityCheck;
use crate::terry::TerryTask;
use crate::EvaluationData;

/// Check that the validator is present.
#[derive(Debug, Default)]
pub struct ValidatorPresent;

impl SanityCheck<TerryTask> for ValidatorPresent {
    fn name(&self) -> &'static str {
        "ValidatorPresent"
    }

    fn pre_hook(&mut self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if task.validator.is_none() {
            eval.add_diagnostic(Diagnostic::warning("Validator not present"))?;
        }
        Ok(())
    }
}
