//! Sanity checks for IOI-like tasks.

use std::path::Path;

use anyhow::Error;

use task_maker_lang::LanguageManager;

use crate::ioi::IOITask;
use crate::sanity_checks::{SanityCheck, SanityChecks};
use crate::{list_files, EvaluationData, UISender};
use std::collections::HashSet;

mod att;
mod sol;
mod statement;
mod subtasks;
mod task;

/// Make a new `SanityChecks` for a IOI task skipping the checks with the provided names.
pub fn get_sanity_checks(skip: &[String]) -> SanityChecks<IOITask> {
    SanityChecks::new(get_sanity_check_list(skip))
}

/// Return the list of sanity checks excluding the ones with their name in the provided list.
fn get_sanity_check_list(skip: &[String]) -> Vec<Box<dyn SanityCheck<IOITask>>> {
    let all: Vec<Box<dyn SanityCheck<_>>> = vec![
        Box::new(task::TaskMaxScore::default()),
        Box::new(task::BrokenSymlinks::default()),
        Box::new(att::AttGraders::default()),
        Box::new(att::AttTemplates::default()),
        Box::new(att::AttSampleFiles::default()),
        Box::new(att::AttSampleFilesValid::default()),
        Box::new(sol::SolGraders::default()),
        Box::new(sol::SolSymlink::default()),
        Box::new(statement::StatementSubtasks::default()),
        Box::new(statement::StatementValid::default()),
        Box::new(statement::StatementCompiled::default()),
        Box::new(statement::StatementGit::default()),
        Box::new(subtasks::MissingSubtaskNames::default()),
        Box::new(subtasks::SolutionsWithNoChecks::default()),
        Box::new(subtasks::InvalidSubtaskName::default()),
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
    task: &IOITask,
    eval: &mut EvaluationData,
    folder: P,
) -> Result<(), Error> {
    if !has_grader(task) {
        return Ok(());
    }
    // some task formats use stub.* others use grader.*
    // To avoid confusion emit warnings only either for stubs or graders.
    let is_stub = task
        .grader_map
        .all_paths()
        .filter_map(|p| p.file_stem())
        .any(|p| p == "stub");
    let mut graders = HashSet::new();
    for file in list_files(task.path.join(folder.as_ref()), vec!["*.*"]) {
        let stem = match file.file_stem() {
            Some(stem) => stem,
            None => continue,
        };
        // do not check the graders
        if stem == "grader" || stem == "stub" {
            continue;
        }
        if let Some(lang) = LanguageManager::detect_language(&file) {
            let ext = lang.extensions()[0];
            if is_stub {
                graders.insert(file.with_file_name(format!("stub.{}", ext)));
            } else {
                graders.insert(file.with_file_name(format!("grader.{}", ext)));
            }
        }
    }
    for grader in graders {
        if !grader.exists() {
            let name = Path::new(grader.file_name().unwrap());
            eval.sender.send_error(format!(
                "Missing grader at {}/{}",
                folder.as_ref().display(),
                name.display()
            ))?;
        }
    }
    Ok(())
}

/// Check if the task uses the graders.
fn has_grader(task: &IOITask) -> bool {
    task.grader_map.all_paths().count() != 0
}
