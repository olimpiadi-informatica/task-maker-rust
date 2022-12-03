use anyhow::{anyhow, Error};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::IOITask;
use crate::sanity_checks::SanityCheck;
use crate::{list_files, EvaluationData};

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
                eval.add_diagnostic(Diagnostic::warning(format!(
                    "Solution {} is not a symlink",
                    task.path_of(&solution).display()
                )))?;
            }
        }
        Ok(())
    }
}

/// Check that the template is a symlink.
#[derive(Debug, Default)]
pub struct SolTemplateSymlink;

impl SanityCheck<IOITask> for SolTemplateSymlink {
    fn name(&self) -> &'static str {
        "SolTemplateSymlink"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
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
