use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Error};
use itertools::Itertools;
use regex::Regex;
use task_maker_diagnostics::{CodeSpan, Diagnostic};

use crate::ioi::{IOITask, SubtaskId};
use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::EvaluationData;

/// Check that the subtasks in the statement are consistent with the ones of the task.
#[derive(Debug, Default)]
pub struct StatementSubtasks;
make_sanity_check!(StatementSubtasks);

impl SanityCheck for StatementSubtasks {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "StatementSubtasks"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let expected_subtasks = task
            .subtasks
            .iter()
            .map(|(st_num, st)| ExtractedSubtask {
                id: *st_num,
                score: Some(st.max_score),
                subtask_id_span: st.span.clone(),
                subtask_score_span: st.span.clone(),
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
            let subtasks = match extract_subtasks(statement_path, source) {
                None => continue,
                Some(subtasks) => subtasks,
            };
            let one_based = (subtasks[0].id == 1) as u32;
            for (expected, actual) in expected_subtasks.iter().zip(subtasks.iter()) {
                let expected_id = expected.id + one_based;
                if expected_id != actual.id {
                    let mut diagnostic = Diagnostic::error(format!(
                        "The subtasks in {} are not sequentially numbered",
                        statement_path.display()
                    ))
                    .with_note(format!(
                        "Expecting subtask {}, found subtask {}",
                        expected_id, actual.id
                    ));
                    if let Some(span) = &actual.subtask_id_span {
                        diagnostic = diagnostic.with_code_span(span.clone());
                    }
                    eval.add_diagnostic(diagnostic)?;
                    break;
                }
                if let Some(actual_score) = actual.score {
                    if approx::abs_diff_ne!(expected.score.unwrap(), actual_score) {
                        let mut diagnostic = Diagnostic::error(format!(
                            "The score of subtask {} in {} doesn't match the task's one",
                            actual.id,
                            statement_path.display()
                        ))
                        .with_note(format!(
                            "Expecting {}, found {}",
                            expected.score.unwrap(),
                            actual_score
                        ));
                        if let Some(span) = &actual.subtask_score_span {
                            diagnostic = diagnostic.with_code_span(span.clone());
                        }
                        eval.add_diagnostic(diagnostic)?;
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
make_sanity_check!(StatementValid);

impl SanityCheck for StatementValid {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "StatementValid"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn post_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        match find_statement_pdf(task) {
            None => {
                let mut diagnostic = Diagnostic::error(
                    "Missing statement file (statement/statement.pdf or testo/testo.pdf)",
                )
                .with_note("Without that file cms will not be able to import the task");
                if let Some(booklet) = task.booklets.first() {
                    let name = booklet.dest.file_name().unwrap();
                    let name = Path::new(name);
                    diagnostic = diagnostic
                        .with_help(format!("Try: ln -s {} testo/testo.pdf", name.display()));
                };
                eval.add_diagnostic(diagnostic)?;
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
make_sanity_check!(StatementCompiled);

impl SanityCheck for StatementCompiled {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "StatementCompiled"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn post_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
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
make_sanity_check!(StatementGit);

impl SanityCheck for StatementGit {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "StatementGit"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn post_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
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
    /// Span of where the subtask id comes from.
    subtask_id_span: Option<CodeSpan>,
    /// Span of where the subtask score comes from.
    subtask_score_span: Option<CodeSpan>,
}

/// Extract from the OII's usual format the subtasks. They are for example:
///
/// `\item \textbf{\makebox[2cm][l]{Subtask 2} [20 punti]}: $L\leq 10$.`
///
/// The regex is pretty powerful and tries to match as many variations as possible.
fn check_subtasks_oii(path: &Path, text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\{(Subtask ([0-9]+))\} *\[((?:\\phantom\{[^\}]+\})?([0-9]+).*)\].*")
                .expect("Invalid regex");
    }
    let mut result = Vec::new();
    for subtask in FIND_SUBTASKS.captures_iter(text) {
        let id_span = subtask.get(1).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        let score_span = subtask.get(3).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        let num = subtask[2].parse::<SubtaskId>();
        let score = subtask[4].parse::<f64>();
        if let (Ok(num), Ok(score)) = (num, score) {
            result.push(ExtractedSubtask {
                id: num,
                score: Some(score),
                subtask_id_span: id_span,
                subtask_score_span: score_span,
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
fn check_subtasks_oii_new(path: &Path, text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex = Regex::new(r"(.*\\subtask.*)").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, captures) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let span = captures.get(1).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        result.push(ExtractedSubtask {
            id: index as SubtaskId,
            score: None,
            subtask_id_span: span,
            subtask_score_span: None,
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
fn check_subtasks_ois(path: &Path, text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex =
            Regex::new(r".*\\OISubtask\{(\d+)\}\{\d+\}\{.+\}.*").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, subtask) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let span = subtask.get(1).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        let score = subtask[1].parse::<f64>();
        if let Ok(score) = score {
            result.push(ExtractedSubtask {
                id: index as SubtaskId,
                score: Some(score),
                subtask_id_span: None,
                subtask_score_span: span,
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

/// Try to extract from the tex file the list of statements. If the list is empty, `None` is
/// returned.
fn extract_subtasks(path: &Path, tex: String) -> Option<Vec<ExtractedSubtask>> {
    let subtasks = if let Some(subtasks) = check_subtasks_oii(path, &tex) {
        subtasks
    } else if let Some(subtasks) = check_subtasks_oii_new(path, &tex) {
        subtasks
    } else {
        check_subtasks_ois(path, &tex)?
    };
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
