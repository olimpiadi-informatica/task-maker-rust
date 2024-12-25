use anyhow::Error;
use task_maker_diagnostics::Diagnostic;

use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::terry::TerryTask;
use crate::EvaluationData;

/// Check that the statement file is present.
#[derive(Debug, Default)]
pub struct StatementPresent;
make_sanity_check!(StatementPresent);

impl SanityCheck for StatementPresent {
    type Task = TerryTask;

    fn name(&self) -> &'static str {
        "StatementPresent"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn pre_hook(&self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if !task.path.join("statement/statement.md").exists()
            && !task.path.join("statement/statement.in.md").exists()
        {
            eval.add_diagnostic(Diagnostic::error(
                "Neither statement/statement.md nor statement/statement.in.md exists",
            ))?;
        }
        Ok(())
    }
}
