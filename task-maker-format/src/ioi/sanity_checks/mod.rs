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
use std::sync::Mutex;

/// Trait that describes the behaviour of a sanity check.
trait SanityCheck: Send + Sync + std::fmt::Debug {
    /// The name of the sanity check.
    fn name(&self) -> &'static str;

    /// This function will be called before the actual execution of the DAG. It can add new
    /// executions to the DAG.
    fn pre_hook(&mut self, _task: &Task, _eval: &mut EvaluationData) -> Result<(), Error> {
        Ok(())
    }

    /// This function will be called after the execution of the DAG completes.
    fn post_hook(&mut self, _task: &Task, _ui: &mut UIMessageSender) -> Result<(), Error> {
        Ok(())
    }
}

/// Internal state of the sanity checks.
#[derive(Debug, Default)]
struct SanityChecksState {
    /// The list of enabled sanity checks.
    sanity_checks: Vec<Box<dyn SanityCheck>>,
}

/// Sanity checks for a IOI task.
#[derive(Debug)]
pub struct SanityChecks {
    /// The internal state of the sanity checks. Mutex to allow interior mutability and Send+Sync
    /// support.
    state: Mutex<SanityChecksState>,
}

impl SanityChecks {
    /// Function called for the first pass of sanity checks of the task. This will check all the
    /// statically checkable properties of the task and may add some executions for checking dynamic
    /// properties of the task.
    pub fn pre_hook(&self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        for check in state.sanity_checks.iter_mut() {
            if let Err(e) = check.pre_hook(task, eval) {
                eval.sender.send(UIMessage::Warning {
                    message: format!("Sanity check {} failed: {}", check.name(), e),
                })?;
            }
        }
        Ok(())
    }

    /// Function called after the evaluation completes. This will check that the produced assets are
    /// valid and the executions added by the pre_hook produced the correct results.
    pub fn post_hook(&self, task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        for check in state.sanity_checks.iter_mut() {
            if let Err(e) = check.post_hook(task, ui) {
                ui.send(UIMessage::Warning {
                    message: format!("Sanity check {} failed: {}", check.name(), e),
                })?;
            }
        }
        Ok(())
    }
}

impl Default for SanityChecks {
    fn default() -> SanityChecks {
        SanityChecks {
            state: Mutex::new(SanityChecksState {
                sanity_checks: vec![
                    Box::new(task::TaskMaxScore::default()),
                    Box::new(task::BrokenSymlinks::default()),
                    Box::new(att::AttGraders::default()),
                    Box::new(att::AttTemplates::default()),
                    Box::new(att::AttSampleFiles::default()),
                    Box::new(sol::SolGraders::default()),
                    Box::new(sol::SolSymlink::default()),
                    Box::new(sol::SolUnique::default()),
                    Box::new(statement::StatementSubtasks::default()),
                    Box::new(statement::StatementValid::default()),
                    Box::new(statement::StatementGit::default()),
                ],
            }),
        }
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
