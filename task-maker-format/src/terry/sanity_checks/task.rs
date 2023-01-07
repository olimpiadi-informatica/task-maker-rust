use anyhow::Error;
use task_maker_diagnostics::Diagnostic;

use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::terry::TerryTask;
use crate::EvaluationData;

/// Check that the validator is present.
#[derive(Debug, Default)]
pub struct ValidatorPresent;
make_sanity_check!(ValidatorPresent, TerryTask);

impl SanityCheck<TerryTask> for ValidatorPresent {
    fn name(&self) -> &'static str {
        "ValidatorPresent"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Io
    }

    fn pre_hook(&self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if task.validator.is_none() {
            eval.add_diagnostic(Diagnostic::warning("Validator not present"))?;
        }
        Ok(())
    }
}
