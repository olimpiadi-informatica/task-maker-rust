use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Error};
use itertools::Itertools;
use regex::Regex;
use task_maker_diagnostics::Diagnostic;

use crate::ioi::{IOITask, SubtaskId};
use crate::sanity_checks::SanityCheck;
use crate::EvaluationData;

/// Check that the subtasks in the statement are consistent with the ones of the task.
#[derive(Debug, Default)]
pub struct StatementSubtasks;

impl SanityCheck<IOITask> for StatementSubtasks {
    fn name(&self) -> &'static str {
        "StatementSubtasks"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let expected_subtasks = task
            .subtasks
            .iter()
            .map(|(st_num, st)| ExtractedSubtask {
                id: *st_num,
                score: Some(st.max_score),
            })
            .sorted_by_key(|st| st.id)
            .collect_vec();
        for booklet in task.booklets.iter() {
            if booklet.statements.len() != 1 {
                continue;
            }
            let statement = &booklet.statements[0];
            let statement_path = task.path_of(&statement.path);
            let source = statement.tex();
            let subtasks = match extract_subtasks(source) {
                None => continue,
                Some(subtasks) => subtasks,
            };
            for (expected, actual) in expected_subtasks.iter().zip(subtasks.iter()) {
                if expected.id != actual.id {
                    eval.add_diagnostic(
                        Diagnostic::error(format!(
                            "The subtasks in {} are not sequentially numbered",
                            statement_path.display()
                        ))
                        .with_note(format!(
                            "Expecting subtask {}, found subtask {}",
                            expected.id, actual.id
                        )),
                    )?;
                    break;
                }
                if let Some(actual_score) = actual.score {
                    if approx::abs_diff_ne!(expected.score.unwrap(), actual_score) {
                        eval.add_diagnostic(
                            Diagnostic::error(format!(
                                "The score of subtask {} in {} doesn't match the task's one",
                                actual.id,
                                statement_path.display()
                            ))
                            .with_note(format!(
                                "Expecting {}, found {}",
                                expected.score.unwrap(),
                                actual_score
                            )),
                        )?;
                        break;
                    }
                }
            }
            if expected_subtasks.len() != subtasks.len() {
                eval.add_diagnostic(
                    Diagnostic::error(format!(
                        "Wrong number of subtasks in {}",
                        statement_path.display()
                    ))
                    .with_note(format!(
                        "Expecting {} subtasks, found {}",
                        expected_subtasks.len(),
                        subtasks.len()
                    )),
                )?;
            }
        }
        Ok(())
    }
}

/// Check that the statement file is valid.
#[derive(Debug, Default)]
pub struct StatementValid;

impl SanityCheck<IOITask> for StatementValid {
    fn name(&self) -> &'static str {
        "StatementValid"
    }

    fn post_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        match find_statement_pdf(task) {
            None => {
                eval.add_diagnostic(
                    Diagnostic::error(
                        "Missing statement file (statement/statement.pdf or testo/testo.pdf)",
                    )
                    .with_note("Without that file cms will not be able to import the task"),
                )?;
            }
            Some(path) => {
                // normal file or valid symlink
                if path.exists() {
                    let mut file = std::fs::File::open(&path).with_context(|| {
                        format!("Failed to open statement file at {}", path.display())
                    })?;
                    let mut buf = [0u8; 4];
                    let invalid = match file.read_exact(&mut buf) {
                        Err(_) => true,
                        Ok(_) => {
                            // check PDF magic number
                            &buf != b"%PDF"
                        }
                    };

                    if invalid {
                        eval.add_diagnostic(Diagnostic::error(format!(
                            "Invalid PDF file at {}",
                            task.path_of(&path).display()
                        )))?;
                    }
                    return Ok(());
                }
                // broken symlink
                else if path.read_link().is_ok() {
                    eval.add_diagnostic(Diagnostic::error(format!(
                        "Statement {} is a broken link",
                        task.path_of(&path).display()
                    )))?;
                }
            }
        }
        Ok(())
    }
}

/// Check that the statement file comes out of the compilation of one of the booklets.
#[derive(Debug, Default)]
pub struct StatementCompiled;

impl SanityCheck<IOITask> for StatementCompiled {
    fn name(&self) -> &'static str {
        "StatementCompiled"
    }

    fn post_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        // If there are no booklets it may mean that the statement is compiled with an external tool
        // or that the statement compilation is not done. Either way this sanity check should be
        // ignored.
        if task.booklets.is_empty() {
            return Ok(());
        }

        let path = match find_statement_pdf(task) {
            Some(path) => path,
            _ => return Ok(()),
        };
        // The source of the actual statement pdf (symlinks resolved). If the symlink is broken,
        // there's nothing we can do (another sanity check will warn this error).
        let target = match path.canonicalize() {
            Ok(path) => path,
            _ => return Ok(()),
        };

        let mut booklet_dest = vec![];
        for booklet in &task.booklets {
            let dest = match booklet.dest.canonicalize() {
                Ok(dest) => dest,
                _ => continue,
            };
            // this booklet corresponds to the official statement file, so we are good!
            if dest == target {
                return Ok(());
            }
            booklet_dest.push(dest);
        }

        // We didn't find any compiled booklet referring to the official statement, this means that
        // the statement that will be used isn't the one compiled by us.
        let booklet_dest = booklet_dest
            .iter()
            .map(|p| task.path_of(p))
            .map(|p| p.to_string_lossy())
            .join(", ");
        eval.add_diagnostic(
            Diagnostic::warning(format!(
                "The official statement at {} is not the one compiled by task-maker",
                task.path_of(&path).display()
            ))
            .with_help(format!(
                "Maybe it should be a symlink to one of the compiled PDF ({})",
                booklet_dest
            )),
        )?;
        Ok(())
    }
}

/// Check that the statement file is known to git.
#[derive(Debug, Default)]
pub struct StatementGit;

impl SanityCheck<IOITask> for StatementGit {
    fn name(&self) -> &'static str {
        "StatementGit"
    }

    fn post_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let path = match find_statement_pdf(task) {
            None => return Ok(()),
            Some(path) => path,
        };
        let path = task.path_of(&path);
        let raw_path = path.as_os_str().as_bytes();
        let mut command = Command::new("git");
        command.arg("ls-files").arg("-z").current_dir(&task.path);
        let output = match command.output() {
            // git not available
            Err(_) => return Ok(()),
            Ok(output) => output,
        };
        // not a git repo
        if !output.status.success() {
            return Ok(());
        }
        // file not know to git
        if !output.stdout.is_empty() && !output.stdout.split(|&b| b == 0).any(|p| p == raw_path) {
            eval.add_diagnostic(
                Diagnostic::error(format!("File {} is not known to git", path.display()))
                    .with_help(format!("Try git add -f {}", path.display())),
            )?;
        }

        Ok(())
    }
}

/// An extracted subtask from the statement file.
struct ExtractedSubtask {
    /// The id of the subtask.
    id: SubtaskId,
    /// The score of the subtask, if present.
    score: Option<f64>,
    // TODO: add span to the source file
}

/// Extract from the OII's usual format the subtasks. They are for example:
///
/// `\item \textbf{\makebox[2cm][l]{Subtask 2} [20 punti]}: $L\leq 10$.`
///
/// The regex is pretty powerful and tries to match as many variations as possible.
fn check_subtasks_oii(text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\{Subtask ([0-9]+)\} *\[(?:\\phantom\{[^\}]+\})?([0-9]+).*\].*")
                .expect("Invalid regex");
    }
    let mut result = Vec::new();
    for subtask in FIND_SUBTASKS.captures_iter(text) {
        let num = subtask[1].parse::<SubtaskId>();
        let score = subtask[2].parse::<f64>();
        if let (Ok(num), Ok(score)) = (num, score) {
            result.push(ExtractedSubtask {
                id: num,
                score: Some(score),
            });
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

/// Extract from the OII's new format the subtasks. They are for example:
///
/// `\item \subtask $L\leq 10$.`
fn check_subtasks_oii_new(text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex = Regex::new(r".*\\subtask.*").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, _) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        result.push(ExtractedSubtask {
            id: index as SubtaskId,
            score: None,
        });
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
fn check_subtasks_ois(text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\\OISubtask\{(\d+)\}\{\d+\}\{.+\}.*").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, subtask) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let score = subtask[1].parse::<f64>();
        if let Ok(score) = score {
            result.push(ExtractedSubtask {
                id: index as SubtaskId,
                score: Some(score),
            });
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
fn extract_subtasks(tex: String) -> Option<Vec<ExtractedSubtask>> {
    let mut subtasks = if let Some(subtasks) = check_subtasks_oii(&tex) {
        subtasks
    } else if let Some(subtasks) = check_subtasks_oii_new(&tex) {
        subtasks
    } else {
        check_subtasks_ois(&tex)?
    };
    // subtasks 1-based
    if subtasks[0].id == 1 {
        for subtask in subtasks.iter_mut() {
            // make the subtasks 0-based
            if subtask.id > 0 {
                subtask.id -= 1;
            }
        }
    }
    Some(subtasks)
}

/// Search for the statement file, returning its path or None if it doesn't exists.
///
/// Will return the path even in case of broken links.
fn find_statement_pdf(task: &IOITask) -> Option<PathBuf> {
    for path in &["statement/statement.pdf", "testo/testo.pdf"] {
        let path = task.path.join(path);
        if path.exists() || path.read_link().is_ok() {
            return Some(path);
        }
    }
    None
}
