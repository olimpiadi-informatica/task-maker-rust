//! Sanity checks for IOI-like tasks.

use crate::ioi::Task;
use crate::ui::{UIMessage, UIMessageSender};
use crate::{list_files, EvaluationData, UISender};
use failure::Error;
use std::path::Path;
use task_maker_lang::LanguageManager;

mod att;
mod sol;
mod statement;
mod task;
use task as task_mod;

#[derive(Debug, Clone, Default)]
struct SanityChecksState;

/// Sanity checks for a IOI task.
#[derive(Debug, Clone, Default)]
pub struct SanityChecks {
    state: SanityChecksState,
}

impl SanityChecks {
    /// Function called for the first pass of sanity checks of the task. This will check all the
    /// statically checkable properties of the task and may add some executions for checking dynamic
    /// properties of the task.
    pub fn pre_hook(&self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        task_mod::check_task_max_score(task, eval)?;
        att::check_att_graders(task, eval)?;
        att::check_att_templates(task, eval)?;
        att::check_att_sample_files(task, eval)?;
        sol::check_sol_graders(task, eval)?;
        sol::check_sol_symlink(task, eval)?;
        sol::check_sol_unique(task, eval)?;
        statement::check_statement_subtasks(task, eval)?;
        Ok(())
    }

    /// Function called after the evaluation completes. This will check that the produced assets are
    /// valid and the executions added by the pre_hook produced the correct results.
    pub fn post_hook(&self, task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
        statement::check_statement_valid(task, ui)?;
        statement::check_statement_git(task, ui)?;
        task_mod::check_broken_symlinks(task, ui)?;
        Ok(())
    }
}

/// Check that all the source file inside `folder` have the corresponding grader, if at least one
/// grader is present in the grader map.
fn check_missing_graders<P: AsRef<Path>>(
    task: &Task,
    eval: &mut EvaluationData,
    folder: P,
) -> Result<(), Error> {
    if !has_grader(task) {
        return Ok(());
    }
    for file in list_files(task.path.join(folder.as_ref()), vec!["*.*"]) {
        let stem = match file.file_stem() {
            Some(stem) => stem,
            None => continue,
        };
        // do not check the graders
        if stem == "grader" {
            continue;
        }
        if let Some(lang) = LanguageManager::detect_language(&file) {
            let ext = lang.extensions()[0];
            let grader = file.with_file_name(format!("grader.{}", ext));
            if !grader.exists() {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "Missing grader at {}/grader.{}",
                        folder.as_ref().display(),
                        ext
                    ),
                })?;
            }
        }
    }
    Ok(())
}

/// Check if the task uses the graders.
fn has_grader(task: &Task) -> bool {
    task.grader_map.all_paths().count() != 0
}
