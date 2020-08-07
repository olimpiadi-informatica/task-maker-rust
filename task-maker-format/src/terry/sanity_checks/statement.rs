use failure::Error;

use crate::sanity_checks::SanityCheck;
use crate::terry::TerryTask;
use crate::ui::UIMessage;
use crate::{EvaluationData, UISender};

/// Check that the statement file is present.
#[derive(Debug, Default)]
pub struct StatementPresent;

impl SanityCheck<TerryTask> for StatementPresent {
    fn name(&self) -> &'static str {
        "StatementPresent"
    }

    fn pre_hook(&mut self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        if !task.path.join("statement/statement.md").exists() {
            eval.sender.send(UIMessage::Warning {
                message: "statement/statement.md does not exist".into(),
            })?;
        }
        Ok(())
    }
}
