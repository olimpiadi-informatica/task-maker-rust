//! Sanity checks for IOI-like tasks.

use std::path::Path;

use anyhow::Error;

use task_maker_lang::LanguageManager;

use crate::ioi::IOITask;
use crate::sanity_checks::{SanityCheck, SanityChecks};
use crate::{list_files, EvaluationData};
use std::collections::HashMap;
use task_maker_diagnostics::Diagnostic;

mod att;
mod checker;
mod io;
mod sol;
mod statement;
mod subtasks;
mod task;

inventory::collect!(&'static dyn SanityCheck<IOITask>);

/// Make a new `SanityChecks` for a IOI task skipping the checks with the provided names.
pub fn get_sanity_checks(skip: &[&str]) -> SanityChecks<IOITask> {
    SanityChecks::new(get_sanity_check_list(skip))
}

/// Return the list of sanity checks excluding the ones with their name in the provided list.
pub fn get_sanity_check_list(skip: &[&str]) -> Vec<&'static dyn SanityCheck<IOITask>> {
    inventory::iter::<&dyn SanityCheck<IOITask>>()
        .cloned()
        .filter(|s| !skip.contains(&s.name()) && !skip.contains(&s.category().as_str()))
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
    let mut by_ext = HashMap::new();
    for file in list_files(task.path.join(folder.as_ref()), vec!["*.*"]) {
        let file = task.path_of(&file);
        let stem = match file.file_stem() {
            Some(stem) => stem,
            None => continue,
        };
        // do not check the graders
        if stem == "grader" || stem == "stub" {
            continue;
        }
        if let Some(lang) = LanguageManager::detect_language(file) {
            let ext = lang.extensions()[0];
            let name = format!("{}.{}", if is_stub { "stub" } else { "grader" }, ext);
            let grader_name = file.with_file_name(name);
            let grader_path = task.path.join(&grader_name);
            by_ext.insert(ext, (grader_path, grader_name, file.to_owned()));
        }
    }
    for (_ext, (grader_path, grader_name, cause_name)) in by_ext {
        if !grader_path.exists() {
            eval.add_diagnostic(
                Diagnostic::error(format!("Missing grader at {}", grader_name.display()))
                    .with_note(format!("Because of {}", cause_name.display())),
            )?;
        }
    }
    Ok(())
}

/// Check if the task uses the graders.
fn has_grader(task: &IOITask) -> bool {
    task.grader_map.all_paths().count() != 0
}
