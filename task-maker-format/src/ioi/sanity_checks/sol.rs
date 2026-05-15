use anyhow::{anyhow, Error};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::IOITask;
use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::{list_files, EvaluationData};

/// Check that all the graders inside sol are present.
#[derive(Debug, Default)]
pub struct SolGraders;
make_sanity_check!(SolGraders);

impl SanityCheck for SolGraders {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "SolGraders"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Solutions
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        check_missing_graders(task, eval, "sol")
    }
}

/// Check that the template is a symlink.
#[derive(Debug, Default)]
pub struct SolTemplateSymlink;
make_sanity_check!(SolTemplateSymlink);

impl SanityCheck for SolTemplateSymlink {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "SolTemplateSymlink"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Solutions
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        for template in list_files(&task.path, vec!["sol/template.*"]) {
            let ext = template
                .extension()
                .ok_or_else(|| anyhow!("Template has no extension"))?
                .to_string_lossy();

            let att_template = format!("att/{}.{}", task.name, ext);

            if !template.is_symlink() {
                eval.add_diagnostic(Diagnostic::warning(format!(
                    "Template {} is not a symlink. It should point to {}",
                    task.path_of(&template).display(),
                    att_template
                )))?;
            }
        }
        Ok(())
    }
}

/// Check that all the solutions (that are not symlinks) contain at least one check.
#[derive(Debug, Default)]
pub struct SolutionsWithNoChecks;
make_sanity_check!(SolutionsWithNoChecks);

impl SanityCheck for SolutionsWithNoChecks {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "SolutionsWithNoChecks"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Solutions
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        // If not all the subtasks have a name, do not bother with the solutions, it's much
        // more important to give everything a name before.
        if task.subtasks.values().any(|st| st.name.is_none()) {
            return Ok(());
        }

        let mut solutions = vec![];
        for solution in eval.solutions.iter() {
            if !solution.checks.is_empty() {
                continue;
            }
            let path = &solution.source_file.path;
            // Ignore the symlinks, since they may come from att/, in which we don't want to put the
            // checks.
            if path.is_symlink() {
                continue;
            }
            solutions.push(format!(
                "{}",
                solution.source_file.relative_path().display()
            ))
        }
        if !solutions.is_empty() {
            eval.add_diagnostic(Diagnostic::warning(format!(
                "The following solutions are missing the subtask checks: {}",
                solutions.join(", ")
            )))?;
        }
        Ok(())
    }
}

/// Check that all the solutions (that are not symlinks) contain at least one check.
#[derive(Debug, Default)]
pub struct SolutionsWithSamplesCheck;
make_sanity_check!(SolutionsWithSamplesCheck);

impl SanityCheck for SolutionsWithSamplesCheck {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "SolutionsWithSamplesChecks"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Solutions
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        // If no first subtask exists, or it has no name, skip this check.
        let Some(Some(samples)) = task.subtasks.get(&0).map(|st| st.name.clone()) else {
            return Ok(());
        };

        let mut solutions = vec![];
        for solution in eval.solutions.iter() {
            let samples_checked = solution
                .checks
                .iter()
                .any(|check| check.subtask_name_pattern == samples);
            if samples_checked {
                solutions.push(format!(
                    "{}",
                    solution.source_file.relative_path().display()
                ))
            }
        }
        if !solutions.is_empty() {
            eval.add_diagnostic(
                Diagnostic::warning(format!(
                    "The following solutions have solution checks for the samples: {}",
                    solutions.join(", ")
                ))
                .with_note("Subtask checks on samples are rarely useful."),
            )?;
        }
        Ok(())
    }
}
