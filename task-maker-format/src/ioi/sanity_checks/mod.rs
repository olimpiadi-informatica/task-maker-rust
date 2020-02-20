//! Sanity checks for IOI-like tasks.

use std::path::Path;

use failure::Error;

use task_maker_lang::LanguageManager;

use crate::ioi::Task;
use crate::sanity_checks::{SanityCheck, SanityChecks};
use crate::ui::UIMessage;
use crate::{list_files, EvaluationData, UISender};

mod att;
mod sol;
mod statement;
mod task;

/// Make a new `SanityChecks` for a IOI task skipping the checks with the provided names.
pub fn get_sanity_checks(skip: &[String]) -> SanityChecks<Task> {
    SanityChecks::new(get_sanity_check_list(skip))
}

/// Return the list of sanity checks excluding the ones with their name in the provided list.
fn get_sanity_check_list(skip: &[String]) -> Vec<Box<dyn SanityCheck<Task>>> {
    let all: Vec<Box<dyn SanityCheck<_>>> = vec![
        Box::new(task::TaskMaxScore::default()),
        Box::new(task::BrokenSymlinks::default()),
        Box::new(att::AttGraders::default()),
        Box::new(att::AttTemplates::default()),
        Box::new(att::AttSampleFiles::default()),
        Box::new(att::AttSampleFilesValid::default()),
        Box::new(sol::SolGraders::default()),
        Box::new(sol::SolSymlink::default()),
        Box::new(sol::SolUnique::default()),
        Box::new(statement::StatementSubtasks::default()),
        Box::new(statement::StatementValid::default()),
        Box::new(statement::StatementGit::default()),
    ];
    all.into_iter()
        .filter(|s| !skip.contains(&s.name().into()))
        .collect()
}

/// Return a comma separated list of the names of all the sanity checks.
pub fn get_sanity_check_names() -> Vec<&'static str> {
    get_sanity_check_list(&[])
        .iter()
        .map(|s| s.name())
        .collect()
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
