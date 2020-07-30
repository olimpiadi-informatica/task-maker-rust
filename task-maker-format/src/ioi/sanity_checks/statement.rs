use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use failure::Error;
use itertools::Itertools;
use regex::Regex;

use crate::ioi::{SubtaskId, Task};
use crate::sanity_checks::SanityCheck;
use crate::ui::{UIMessage, UIMessageSender};
use crate::{EvaluationData, UISender};

/// Check that the subtasks in the statement are consistent with the ones of the task.
#[derive(Debug, Default)]
pub struct StatementSubtasks;

impl SanityCheck<Task> for StatementSubtasks {
    fn name(&self) -> &'static str {
        "StatementSubtasks"
    }

    fn pre_hook(&mut self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let expected_subtasks = task
            .subtasks
            .iter()
            .map(|(st_num, st)| (*st_num, st.max_score))
            .sorted_by_key(|(st, _)| *st)
            .collect_vec();
        for booklet in task.booklets.iter() {
            if booklet.statements.len() != 1 {
                continue;
            }
            let statement = &booklet.statements[0];
            let source = statement.tex();
            let subtasks = match extract_subtasks(source) {
                None => continue,
                Some(subtasks) => subtasks,
            };
            let mut non_sequential = false;
            let mut wrong = false;
            for (expected, actual) in expected_subtasks.iter().zip(subtasks.iter()) {
                if expected.0 != actual.0 {
                    non_sequential = true;
                    break;
                }
                if approx::abs_diff_ne!(expected.1, actual.1) {
                    wrong = true;
                    break;
                }
            }
            if expected_subtasks.len() != subtasks.len() {
                wrong = true;
            }
            if non_sequential {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "The subtasks in the statement {} are non-sequentially numbered",
                        statement.path.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            } else if wrong {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "The subtasks in the statement {} don't match the tasks's ones",
                        statement.path.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        }
        Ok(())
    }
}

/// Check that the statement file is valid.
#[derive(Debug, Default)]
pub struct StatementValid;

impl SanityCheck<Task> for StatementValid {
    fn name(&self) -> &'static str {
        "StatementValid"
    }

    fn post_hook(&mut self, task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
        match find_statement_pdf(task) {
            None => {
                return ui.send(UIMessage::Warning {
                    message: "Missing statement file (statement/statement.pdf or testo/testo.pdf)"
                        .into(),
                });
            }
            Some(path) => {
                // normal file or valid symlink
                if path.exists() {
                    let mut file = std::fs::File::open(&path)?;
                    let mut buf = [0u8; 4];
                    let invalid = match file.read_exact(&mut buf) {
                        Err(_) => true,
                        Ok(_) => {
                            // check PDF magic number
                            &buf != b"%PDF"
                        }
                    };

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
}

/// Check that the statement file is known to git.
#[derive(Debug, Default)]
pub struct StatementGit;

impl SanityCheck<Task> for StatementGit {
    fn name(&self) -> &'static str {
        "StatementGit"
    }

    fn post_hook(&mut self, task: &Task, ui: &mut UIMessageSender) -> Result<(), Error> {
        match find_statement_pdf(task) {
            None => return Ok(()),
            Some(path) => {
                let path = path.strip_prefix(&task.path).unwrap();
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
                                message: format!("File {} is not known to git", path.display()),
                            })?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Extract from the OII's usual format the subtasks. They are for example:
///
/// `\item \textbf{\makebox[2cm][l]{Subtask 2} [20 punti]}: $L\leq 10$.`
///
/// The regex is pretty powerful and tries to match as many variations as possible.
fn check_subtasks_oii(text: &str) -> Option<Vec<(SubtaskId, f64)>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\{Subtask ([0-9]+)\} *\[(?:\\phantom\{.\})?([0-9]+).*\].*")
                .expect("Invalid regex");
    }
    let mut result = Vec::new();
    for subtask in FIND_SUBTASKS.captures_iter(text) {
        let num = subtask[1].parse::<SubtaskId>();
        let max_score = subtask[2].parse::<f64>();
        if let (Ok(num), Ok(max_score)) = (num, max_score) {
            result.push((num, max_score));
        } else {
            return None;
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Extract from the OIS's usual format the subtasks. They are for example:
///
/// `\OISubtask{10}{1}{$N \le 10$.}`
fn check_subtasks_ois(text: &str) -> Option<Vec<(SubtaskId, f64)>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\\OISubtask\{(\d+)\}\{\d+\}\{.+\}.*").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, subtask) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let max_score = subtask[1].parse::<f64>();
        if let Ok(max_score) = max_score {
            result.push((index as SubtaskId, max_score));
        } else {
            return None;
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Try to extract from the tex file the list of statements, starting with zero. If the list is
/// empty, `None` is returned.
fn extract_subtasks(tex: String) -> Option<Vec<(SubtaskId, f64)>> {
    let mut subtasks = if let Some(subtasks) = check_subtasks_oii(&tex) {
        subtasks
    } else {
        check_subtasks_ois(&tex)?
    };
    // subtasks 1-based
    if subtasks[0].0 == 1 {
        for subtask in subtasks.iter_mut() {
            // make the subtasks 0-based
            if subtask.0 > 0 {
                subtask.0 -= 1;
            }
        }
    }
    Some(subtasks)
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
