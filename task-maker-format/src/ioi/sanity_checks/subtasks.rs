use anyhow::Error;
use itertools::Itertools;
use task_maker_diagnostics::Diagnostic;

use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::{EvaluationData, IOITask};

/// Check that all the subtasks have a name.
#[derive(Debug, Default)]
pub struct MissingSubtaskNames;
make_sanity_check!(MissingSubtaskNames, IOITask);

impl SanityCheck<IOITask> for MissingSubtaskNames {
    fn name(&self) -> &'static str {
        "MissingSubtaskNames"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Io
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut missing_name = vec![];
        for subtask_id in task.subtasks.keys().sorted() {
            let subtask = &task.subtasks[subtask_id];
            if subtask.name.is_none() {
                missing_name.push((
                    format!("Subtask {} ({} points)", subtask.id, subtask.max_score),
                    subtask.span.clone(),
                ));
            }
        }
        if !missing_name.is_empty() {
            let message = format!(
                "These subtasks are missing a name: {}",
                missing_name.iter().map(|(name, _)| name).join(", ")
            );
            let mut diagnostic = Diagnostic::warning(message);
            if missing_name.iter().any(|(_, span)| span.is_some()) {
                diagnostic = diagnostic
                    .with_help("Add '#STNAME: name' in gen/GEN after each subtask definition:");
            }
            for (_, span) in missing_name {
                if let Some(span) = span {
                    diagnostic = diagnostic.with_code_span(span);
                }
            }
            eval.add_diagnostic(diagnostic)?;
        }
        Ok(())
    }
}

/// Check that all the checks target at least one subtask.
#[derive(Debug, Default)]
pub struct InvalidSubtaskName;
make_sanity_check!(InvalidSubtaskName, IOITask);

impl SanityCheck<IOITask> for InvalidSubtaskName {
    fn name(&self) -> &'static str {
        "InvalidSubtaskName"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Io
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let subtask_names = task
            .subtasks
            .keys()
            .sorted()
            .filter_map(|st| task.subtasks[st].name.as_ref())
            .join(", ");
        for solution in &eval.solutions {
            for check in &solution.checks {
                let subtasks = task.find_subtasks_by_pattern_name(&check.subtask_name_pattern);
                if subtasks.is_empty() {
                    eval.add_diagnostic(
                        Diagnostic::error(format!(
                            "Invalid subtask name '{}' in solution '{}'",
                            check.subtask_name_pattern,
                            solution.source_file.relative_path().display()
                        ))
                        .with_note(format!("The valid names are: {}", subtask_names)),
                    )?;
                }
            }
        }
        Ok(())
    }
}
