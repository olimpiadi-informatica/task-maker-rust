use anyhow::Error;
use task_maker_diagnostics::Diagnostic;

use crate::sanity_checks::{make_sanity_check, SanityCheck};
use crate::terry::TerryTask;
use crate::EvaluationData;

/// Check that the statement file is present.
#[derive(Debug, Default)]
pub struct StatementPresent;
make_sanity_check!(StatementPresent);

impl SanityCheck<TerryTask> for StatementPresent {
    fn name(&self) -> &'static str {
        "StatementPresent"
    }

    fn pre_hook(&self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if !task.path.join("statement/statement.md").exists() {
            eval.add_diagnostic(Diagnostic::error("statement/statement.md does not exist"))?;
        }
        Ok(())
    }
}
