use crate::ioi::Task;
use crate::ui::{UIMessage, UIMessageSender};
use crate::{list_files, EvaluationData, UISender};
use failure::Error;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use task_maker_lang::LanguageManager;

/// The default maximum score of a task.
const DEFAULT_TASK_MAX_SCORE: f64 = 100.0;

/// Function called for the first pass of sanity checks of the task.
pub fn pre_hook(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    check_task_max_score(task, eval)?;
    check_att_graders(task, eval)?;
    check_att_templates(task, eval)?;
    check_att_sample_files(task, eval)?;
    check_sol_graders(task, eval)?;
    check_sol_symlink(task, eval)?;
    check_sol_unique(task, eval)?;
    Ok(())
}

/// Function called after the evaluation completes.
pub fn post_hook(task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
    check_statement_valid(task, ui)?;
    check_statement_git(task, ui)?;
    check_broken_symlinks(task, ui)?;
    Ok(())
}

/// Check that the task has the usual maximum score.
fn check_task_max_score(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let task_score: f64 = task.subtasks.values().map(|st| st.max_score).sum();
    if approx::abs_diff_ne!(task_score, DEFAULT_TASK_MAX_SCORE) {
        eval.sender.send(UIMessage::Warning {
            message: format!(
                "The score of the task is {} (not {})",
                task_score, DEFAULT_TASK_MAX_SCORE
            ),
        })?;
    }
    Ok(())
}

/// Check that all the graders are present inside att.
fn check_att_graders(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    check_missing_graders(task, eval, "att")
}

/// Check that all the templates are present inside att.
fn check_att_templates(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    for grader in task.grader_map.all_paths() {
        let ext = grader.extension().unwrap().to_string_lossy();
        let template = task.path.join("att").join(format!("{}.{}", task.name, ext));
        if !template.exists() {
            eval.sender.send(UIMessage::Warning {
                message: format!("Missing template at att/{}.{}", task.name, ext),
            })?;
        }
    }
    Ok(())
}

/// Check that the sample cases inside att are valid symlinks.
fn check_att_sample_files(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let mut no_sample = true;
    for sample in list_files(&task.path, vec!["att/*input*.txt", "att/*output*.txt"]) {
        no_sample = false;
        if let Ok(path) = sample.read_link() {
            if !path.exists() {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "Sample case {} is a broken link",
                        sample.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        } else {
            eval.sender.send(UIMessage::Warning {
                message: format!(
                    "Sample case {} is not a symlink",
                    sample.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    if no_sample {
        eval.sender.send(UIMessage::Warning {
            message: format!("No sample file in att/"),
        })?;
    }
    Ok(())
}

/// Check that all the graders inside sol are present.
fn check_sol_graders(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    check_missing_graders(task, eval, "sol")
}

/// Check that the official solution is a symlink.
fn check_sol_symlink(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    for solution in list_files(&task.path, vec!["sol/solution.*", "sol/soluzione.*"]) {
        if solution.read_link().is_err() {
            eval.sender.send(UIMessage::Warning {
                message: format!(
                    "Solution {} is not a symlink",
                    solution.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    Ok(())
}

/// Check that the official solution is unique.
fn check_sol_unique(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let solutions: Vec<_> = list_files(&task.path, vec!["sol/solution.*", "sol/soluzione.*"])
        .into_iter()
        .map(|s| s.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    if solutions.len() > 1 {
        eval.sender.send(UIMessage::Warning {
            message: format!("More than an official solution found: {:?}", solutions),
        })?;
    }
    Ok(())
}

/// Check that the statement file is valid.
fn check_statement_valid(task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
    match find_statement_pdf(task) {
        None => {
            return ui.send(UIMessage::Warning {
                message: format!(
                    "Missing statement file (statement/statement.pdf or testo/testo.pdf)"
                ),
            });
        }
        Some(path) => {
            // normal file or valid symlink
            if path.exists() {
                let mut file = std::fs::File::open(&path)?;
                let mut buf = [0u8; 4];
                let mut invalid = file.read_exact(&mut buf).is_err();
                // check PDF magic number
                if buf != "%PDF".as_bytes() {
                    invalid = true;
                }
                if invalid {
                    return ui.send(UIMessage::Warning {
                        message: format!(
                            "Invalid PDF file at {}",
                            path.strip_prefix(&task.path).unwrap().display()
                        ),
                    });
                }
                return Ok(());
            }
            // broken symlink
            else if path.read_link().is_ok() {
                return ui.send(UIMessage::Warning {
                    message: format!(
                        "Statement {} is a broken link",
                        path.strip_prefix(&task.path).unwrap().display()
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Check that the statement file is known to git.
fn check_statement_git(task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
    match find_statement_pdf(task) {
        None => return Ok(()),
        Some(path) => {
            let mut command = Command::new("git");
            command
                .arg("ls-files")
                .arg("--")
                .arg(&path)
                .current_dir(&task.path);
            match command.output() {
                // git not available
                Err(_) => return Ok(()),
                Ok(output) => {
                    // not a git repo
                    if !output.status.success() {
                        return Ok(());
                    }
                    // file not know to git
                    if output.stdout.is_empty() {
                        ui.send(UIMessage::Warning {
                            message: format!(
                                "File {} is not known to git",
                                path.strip_prefix(&task.path).unwrap().display()
                            ),
                        })?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Check that there are no broken links.
fn check_broken_symlinks(task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
    for file in list_files(&task.path, vec!["**/*"]) {
        if !file.exists() && file.read_link().is_ok() {
            ui.send(UIMessage::Warning {
                message: format!(
                    "{} is a broken link",
                    file.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    Ok(())
}

/// Search for the statement file, returning its path or None if it doesn't exists.
///
/// Will return the path even in case of broken links.
fn find_statement_pdf(task: &Task) -> Option<PathBuf> {
    for path in &["statement/statement.pdf", "testo/testo.pdf"] {
        let path = task.path.join(path);
        if path.exists() || path.read_link().is_ok() {
            return Some(path);
        }
    }
    None
}

/// Check if the task uses the graders.
fn has_grader(task: &Task) -> bool {
    task.grader_map.all_paths().count() != 0
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
        let stem = file.file_stem().unwrap();
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
