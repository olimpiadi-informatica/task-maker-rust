use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Error};
use itertools::Itertools;
use regex::Regex;
use task_maker_diagnostics::{CodeSpan, Diagnostic};

use crate::ioi::{get_language_from_extension, IOITask, SubtaskId, LANGUAGES};
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

            let Some(lang) = &booklet.lang else {
                continue;
            };
            let builder = get_language_from_extension(lang)?;

            let statement = &booklet.statements[0];
            let statement_path = task.path_of(&statement.path);
            let source = builder.build_statement_source(statement);
            let subtasks = match extract_subtasks(statement_path, &source) {
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

/// Check that there is at least a statement file, and that all statement
/// files are valid
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
        let mut found_valid_statement = false;

        let check_statement = |path: &Path| -> Result<bool, Error> {
            // normal file or valid symlink
            if path.exists() {
                if check_valid_pdf(path)? {
                    return Ok(true);
                } else {
                    eval.add_diagnostic(Diagnostic::error(format!(
                        "Invalid PDF file at {}",
                        task.path_of(path).display()
                    )))?;
                }
            }
            // broken symlink
            else if path.read_link().is_ok() {
                eval.add_diagnostic(Diagnostic::error(format!(
                    "Statement {} is a broken link",
                    task.path_of(path).display()
                )))?;
            }
            Ok(false)
        };

        if let Some(path) = find_statement_pdf(task) {
            eval.add_diagnostic(
                Diagnostic::warning(format!(
                    "Found statement at {}",
                    task.path_of(&path).display()
                ))
                .with_note("This is deprecated, use a language specific statement instead"),
            )?;

            found_valid_statement |= check_statement(&path)?;
        }

        for language in LANGUAGES {
            if let Some(path) = find_language_statement_pdf(task, language) {
                found_valid_statement |= check_statement(&path)?;
            }
        }

        if !found_valid_statement {
            eval.add_diagnostic(
                Diagnostic::error("There is no functioning statement file").with_note(format!(
                    "Consider adding a statement in any of the languages supported by CMS ({})",
                    LANGUAGES.join(", ")
                )),
            )?;
        }

        Ok(())
    }
}

/// Check that the statement files come out of the compilation of one of the booklets,
/// or that they are at least known to git
#[derive(Debug, Default)]
pub struct StatementCompiledOrGit;
make_sanity_check!(StatementCompiledOrGit);

impl SanityCheck for StatementCompiledOrGit {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "StatementCompiledOrGit"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Statement
    }

    fn post_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        // the statements compiled by us
        let booklet_dest = task
            .booklets
            .iter()
            .map(|booklet| booklet.dest.canonicalize())
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        let booklet_dest_list = booklet_dest
            .iter()
            .map(|p| task.path_of(p))
            .map(Path::to_string_lossy)
            .join(", ");

        let check_statement = |path: &PathBuf| -> Result<(), Error> {
            // The file is a symlink but it not known to git
            if path.is_symlink() && !check_known_to_git(task, path)? {
                eval.add_diagnostic(
                    Diagnostic::error(format!(
                        "The official statement at {} is a symbolic link and not known to git",
                        task.path_of(path).display()
                    ))
                    .with_note(
                        "This means that it won't be available outside of your local directory",
                    )
                    .with_help(format!("Try git add -f {}", task.path_of(path).display())),
                )?;
            }

            // If the file is a broken symlink, we cannot check anything.
            // Another sanity check will warn the issue.
            let Ok(target) = &path.canonicalize() else {
                return Ok(());
            };

            let relative_target = resolve_symlink(path)?;

            if booklet_dest.contains(target) {
                return Ok(());
            }

            // We didn't find any compiled booklet referring to the official statement, this means that
            // the statement that will be used isn't the one compiled by us.

            eval.add_diagnostic(
                Diagnostic::warning(format!(
                    "The official statement at {} is not the one compiled by task-maker",
                    task.path_of(target).display()
                ))
                .with_help(format!(
                    "Maybe it should be a symlink to one of the compiled PDF ({})",
                    booklet_dest_list
                )),
            )?;

            if check_known_to_git(task, task.path_of(&relative_target))? {
                return Ok(());
            }

            // The statement is not known to git

            eval.add_diagnostic(
                Diagnostic::error(format!(
                    "The official statement at {} is not compiled by task-maker and not known to git",
                    task.path_of(&relative_target).display()
                ))
                .with_note("This means that it won't be available outside of your local directory")
                .with_help(format!("Try git add -f {}", task.path_of(&relative_target).display()))
            )?;

            Ok(())
        };

        if let Some(path) = find_statement_pdf(task) {
            check_statement(&path)?;
        }

        for language in LANGUAGES {
            if let Some(path) = find_language_statement_pdf(task, language) {
                check_statement(&path)?;
            }
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
            Regex::new(r".*\\(?:OISubtask|IIOTsubtask)\{(\d+)\}\{\d+\}\{.+\}.*")
                .expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, subtask) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let span = subtask.get(1).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        let Ok(score) = subtask[1].parse::<f64>() else {
            return None;
        };
        result.push(ExtractedSubtask {
            id: index as SubtaskId,
            score: Some(score),
            subtask_id_span: None,
            subtask_score_span: span,
        });
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Extract from the usual format for typst the subtasks. They are for example:
///
/// `subtask => [Samples],`
fn check_subtasks_typst(path: &Path, text: &str) -> Option<Vec<ExtractedSubtask>> {
    lazy_static! {
        static ref FIND_SUBTASKS: Regex = Regex::new(r"subtask => \[.+\]").expect("Invalid regex");
    }
    let mut result = Vec::new();
    for (index, subtask) in FIND_SUBTASKS.captures_iter(text).enumerate() {
        let span = subtask.get(1).and_then(|span| {
            CodeSpan::from_str(path, text, span.start(), span.end() - span.start()).ok()
        });
        result.push(ExtractedSubtask {
            id: index as SubtaskId,
            score: None,
            subtask_id_span: None,
            subtask_score_span: span,
        });
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Try to extract from the tex file the list of statements. If the list is empty, `None` is
/// returned.
fn extract_subtasks(path: &Path, tex: &str) -> Option<Vec<ExtractedSubtask>> {
    check_subtasks_oii(path, tex)
        .or_else(|| check_subtasks_oii_new(path, tex))
        .or_else(|| check_subtasks_ois(path, tex))
        .or_else(|| check_subtasks_typst(path, tex))
}

fn resolve_symlink(path: &Path) -> Result<PathBuf, Error> {
    let mut path = path.to_path_buf();
    let mut depth = 0;
    while path.is_symlink() {
        if depth >= 40 {
            bail!("Too many level of symbolic links");
        }
        path = path.read_link()?;
        depth += 1;
    }
    Ok(path)
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

/// Search for a language-specific statement file, returning its path or None if it doesn't exists.
///
/// Will return the path even in case of broken links.
fn find_language_statement_pdf(task: &IOITask, language: &str) -> Option<PathBuf> {
    for path in &[
        format!("statement/{language}.pdf"),
        format!("testo/{language}.pdf"),
    ] {
        let path = task.path.join(path);
        if path.exists() || path.read_link().is_ok() {
            return Some(path);
        }
    }
    None
}

/// Checks whether a file is a valid PDF file
fn check_valid_pdf(path: &Path) -> Result<bool, Error> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open statement file at {}", path.display()))?;
    let mut buf = [0u8; 4];

    let valid = match file.read_exact(&mut buf) {
        Err(_) => false,
        Ok(_) => {
            // check PDF magic number
            &buf == b"%PDF"
        }
    };

    Ok(valid)
}

/// Checks whether a file is known to git
///
/// If git is not present, there is no git repository, or no file is tracked at all
/// this will behave as if the file is known.
fn check_known_to_git(task: &IOITask, path: &Path) -> Result<bool, Error> {
    let raw_path = path.as_os_str().as_bytes();

    let mut command = Command::new("git");
    command.arg("ls-files").arg("-z").current_dir(&task.path);

    let Ok(output) = command.output() else {
        // git is not available
        return Ok(true);
    };

    // not a git repo
    if !output.status.success() {
        return Ok(true);
    }

    let known = output.stdout.is_empty() || output.stdout.split(|&b| b == 0).any(|p| p == raw_path);

    Ok(known)
}
