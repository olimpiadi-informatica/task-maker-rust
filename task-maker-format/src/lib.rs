//! Task parsing and execution using computation DAGs.
//!
//! This crate allows you to parse the tasks on disk and evaluate the solutions inside of them by
//! adding the executions inside an [`ExecutionDAG`](task_maker_dag/struct.ExecutionDAG.html).
//!
//! This crate also provides ui functionalities for showing the progress and the results of the
//! execution.

#![deny(missing_docs)]
#![allow(clippy::upper_case_acronyms)]

#[macro_use]
extern crate approx;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate pest_derive;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Error;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub use detect_format::find_task;
pub use sanity_checks::get_sanity_check_list;
pub use sanity_checks::SanityCheckCategory;
pub use source_file::SourceFile;
pub use tag::{Tag, VALID_TAGS};
pub use task_format::*;
use task_maker_dag::ExecutionDAG;
use task_maker_diagnostics::Diagnostic;
use task_maker_lang::{GraderMap, LanguageManager};

use crate::ioi::task_info::IOITaskInfo;
use crate::ioi::IOITask;
pub use crate::solution::*;
use crate::terry::{Seed, TerryTask};
use crate::ui::UI;
pub use testcase_score_status::ScoreStatus;

mod detect_format;
pub mod ioi;
mod sanity_checks;
mod solution;
mod source_file;
mod tag;
mod task_format;
pub mod terry;
mod testcase_score_status;
pub mod ui;

lazy_static! {
    /// Directory where the data files are stored. It is taken from the `TM_DATA_DIR` environment
    /// variable if present, otherwise it will be defaulted to the path of the source tree.
    pub static ref DATA_DIR: PathBuf = {
        if let Some(dir) = option_env!("TM_DATA_DIR") {
            dir.into()
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("Invalid CARGO_MANIFEST_DIR")
                .join("data")
        }
    };
}

/// Information about a parsed task, returned with the `--task-info` option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskInfo {
    /// The task is IOI-like.
    IOI(IOITaskInfo),
    /// The task is Terry-like.
    Terry(terry::task_info::TerryTaskInfo),
}

/// Configuration of the evaluation of a task.
#[derive(Debug, Clone, Default)]
pub struct EvaluationConfig {
    /// Execute only the solutions whose names start with the filter. If left empty all the
    /// solutions are executed.
    pub solution_filter: Vec<String>,
    /// Include the solutions in the booklet.
    pub booklet_solutions: bool,
    /// Do not build the statement files and the booklets.
    pub no_statement: bool,
    /// Execute only the solution with the specified paths, that can reside anywhere in the
    /// filesystem.
    pub solution_paths: Vec<PathBuf>,
    /// List of disabled sanity check names.
    pub disabled_sanity_checks: Vec<String>,
    /// Force this seed in terry evaluations.
    pub seed: Option<Seed>,
    /// Do not write any file inside the task directory.
    pub dry_run: bool,
}

/// The data for an evaluation, including the DAG and the UI channel.
pub struct EvaluationData {
    /// Root directory of the task.
    pub task_root: PathBuf,
    /// The DAG with the evaluation data.
    pub dag: ExecutionDAG,
    /// The list of solutions to evaluate.
    pub solutions: Vec<Solution>,
    /// The sender of the UI.
    pub sender: Arc<Mutex<ui::UIMessageSender>>,
}

impl EvaluationData {
    /// Crate a new `EvaluationData` returning the data and the receiving part of the UI channel.
    pub fn new<P: Into<PathBuf>>(task_root: P) -> (EvaluationData, ui::UIChannelReceiver) {
        let (sender, receiver) = ui::UIMessageSender::new();
        (
            EvaluationData {
                task_root: task_root.into(),
                dag: ExecutionDAG::new(),
                solutions: Default::default(),
                sender: Arc::new(Mutex::new(sender)),
            },
            receiver,
        )
    }

    /// Add a diagnostic message to the UI.
    pub fn add_diagnostic(&self, diagnostic: Diagnostic) -> Result<(), Error> {
        self.sender.add_diagnostic(diagnostic)
    }
}

/// What can send [`UIMessage`](ui/enum.UIMessage.html)s.
pub trait UISender {
    /// Send that `UIMessage` to the UI.
    fn send(&self, message: ui::UIMessage) -> Result<(), Error>;

    /// Send a diagnostic message to the UI.
    fn add_diagnostic(&self, diagnostic: Diagnostic) -> Result<(), Error> {
        self.send(ui::UIMessage::Diagnostic { diagnostic })
    }
}

/// Implement `.send(message)` for `Mutex<UIMessageSender>` in order to do
/// `EvaluationData.sender.send(message)`. This will lock the mutex and send the message to the UI.
impl UISender for Mutex<ui::UIMessageSender> {
    fn send(&self, message: ui::UIMessage) -> Result<(), Error> {
        self.lock().unwrap().send(message)
    }
}

impl EvaluationConfig {
    /// Returns the solution filters as a vector of strings with the file names of provided
    /// patterns.
    fn solution_filters(&self) -> Vec<String> {
        self.solution_filter
            .iter()
            .map(|filter| {
                // unfortunate lossy cast to String because currently OsString doesn't
                // support .starts_with
                PathBuf::from(filter)
                    .file_name()
                    .expect("Invalid filter provided")
                    .to_string_lossy()
                    .to_string()
            })
            .collect_vec()
    }

    /// Returns the fixed solutions in the config or, if none is specified, all the ones matching
    /// the provided pattern in the provided base directory.
    fn solution_paths(&self, base_dir: &Path, patterns: Vec<&str>) -> Vec<PathBuf> {
        if self.solution_paths.is_empty() {
            list_files(base_dir, patterns)
        } else {
            self.solution_paths.clone()
        }
    }

    /// Search all the solutions matching the provided pattern in the provided base directory,
    /// excluding all the graders in the grader_map, if provided.
    ///
    /// If the configuration is set with a filter, it is applied.
    ///
    /// If the configuration is set to evaluate only some solutions, it is applied.
    pub fn find_solutions(
        &self,
        base_dir: &Path,
        patterns: Vec<&str>,
        grader_map: Option<Arc<GraderMap>>,
        eval: &mut EvaluationData,
    ) -> Vec<Solution> {
        let solutions_paths = self.solution_paths(base_dir, patterns);
        let filter = self.solution_filters();
        let graders: HashSet<PathBuf> = if let Some(grader_map) = &grader_map {
            grader_map.all_paths().map(|p| p.to_path_buf()).collect()
        } else {
            HashSet::new()
        };
        solutions_paths
            .into_iter()
            .filter(|p| !graders.contains(p)) // the graders are not solutions
            .filter(|p| p.exists())
            .filter(|p| {
                if self.solution_filter.is_empty() {
                    return true;
                }
                let name = p.file_name().unwrap().to_string_lossy();
                filter
                    .iter()
                    .any(|filter| name.starts_with(filter.as_str()))
            })
            .filter_map(|path| Solution::new(&path, base_dir, grader_map.clone(), eval))
            .collect()
    }
}

/// List all the files inside `cwd` that matches a list of glob patterns. The results are in the
/// same order of the patterns.
pub(crate) fn list_files<P: AsRef<Path>, S: AsRef<str>>(cwd: P, patterns: Vec<S>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for pattern in patterns.into_iter() {
        let pattern = cwd.as_ref().join(pattern.as_ref());
        for path in glob::glob(&pattern.to_string_lossy())
            .expect("Invalid pattern for list_files")
            .flatten()
        {
            results.push(path);
        }
    }
    results
}

/// Information about where to write the binary of the `SourceFile` found by `find_source_file`.
pub enum WriteBinTo {
    /// Do not write the binary anywhere.
    None,
    /// Write the binary to a file in the same place as the source file, but without extension.
    WithoutExtension,
    /// Write the binary to this path, relative to the base path.
    Path(PathBuf),
}

impl WriteBinTo {
    /// Make a `WriteBinTo::Path`.
    pub fn path<P: Into<PathBuf>>(path: P) -> Self {
        Self::Path(path.into())
    }
}

/// Make a `SourceFile` with each file that match the patterns provided, that is in a recognised
/// language.
///
/// The file name is appended to `description_prefix` and used as description for the source file.
pub(crate) fn find_source_file<
    CwdPath: AsRef<Path>,
    Pattern: AsRef<str>,
    BasePath: Into<PathBuf>,
    S: AsRef<str>,
>(
    cwd: CwdPath,
    patterns: Vec<Pattern>,
    base_path: BasePath,
    description_prefix: S,
    grader_map: Option<Arc<GraderMap>>,
    write_bin_to: WriteBinTo,
) -> Vec<SourceFile> {
    let mut result = vec![];
    let base_path = base_path.into();
    for path in list_files(cwd, patterns) {
        if path.exists() && LanguageManager::detect_language(&path).is_some() {
            let write_bin_to = match &write_bin_to {
                WriteBinTo::None => None,
                WriteBinTo::WithoutExtension => Some(path.with_extension("")),
                WriteBinTo::Path(path) => Some(base_path.join(path)),
            };
            let name = path.strip_prefix(&base_path).unwrap_or(&path);
            // SourceFile::new may fail if the language is unknown
            result.push(
                SourceFile::new(
                    &path,
                    &base_path,
                    format!("{} {}", description_prefix.as_ref(), name.display()),
                    grader_map.clone(),
                    write_bin_to,
                )
                .unwrap(),
            );
        }
    }
    result
}

/// Bind the start/done/skip callbacks of an execution to a ui message sender which sends to the UI
/// messages with the correct status field.
///
/// It's also sent to the UI the message with status `UIExecutionStatus::Pending`.
///
/// It works by first cloning the `extra` arguments for each callback. This is required because each
/// callback has to move inside the needed data. For the same reason also the `UIMessageSender` is
/// cloned and then moved inside the callback. The callbacks then simply send to the UI the value
/// returned by the `enum` lambda.
///
/// # Parameters
/// - `eval: EvaluationData`
/// - `exec_uuid: ExecutionUuid`
/// - `enum` is a lambda that takes one or more arguments:
///   - the first is a `UIExecutionStatus`
///   - the followings are clones of the `extra` parameter
/// - `extra` is a series of identifiers of `Clone`able variables.
#[macro_export]
macro_rules! bind_exec_callbacks {
    ($eval:expr, $exec_uuid:expr, $enum:expr $(,$extra:ident)*) => {
        {
            #[allow(clippy::redundant_closure_call)]
            {
                use $crate::UISender;
                use $crate::ui::UIExecutionStatus;
                {
                    $(let $extra = $extra.clone();)*
                    let status = UIExecutionStatus::Pending;
                    $eval
                        .sender
                        .send(($enum)(status, $($extra,)*))?;
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_start(&$exec_uuid, move |worker| {
                        let status = UIExecutionStatus::Started { worker };
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_done(&$exec_uuid, move |result| {
                        let status = UIExecutionStatus::Done { result };
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_skip(&$exec_uuid, move || {
                        let status = UIExecutionStatus::Skipped;
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
            }
            Result::<(), Error>::Ok(())
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_files() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/bar")).unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/baz")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/yyy.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/yyy.aaa"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/bar/zzz.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("uuu.bbb"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/baz/uuu.bbb"), "x").unwrap();
        let files = list_files(tmpdir.path(), vec!["**/*.py", "foo/baz/*.bbb"]);
        assert_eq!(files.len(), 4);
        assert!(files.contains(&tmpdir.path().join("foo/xxx.py")));
        assert!(files.contains(&tmpdir.path().join("foo/yyy.py")));
        assert!(files.contains(&tmpdir.path().join("foo/bar/zzz.py")));
        assert!(files.contains(&tmpdir.path().join("foo/baz/uuu.bbb")));
    }

    #[test]
    fn test_find_source_file() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/bar")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/bar/zzz.py"), "x").unwrap();
        let mut source = find_source_file(
            tmpdir.path(),
            vec!["foo/bar/*.py"],
            "",
            "",
            None,
            WriteBinTo::None,
        );
        assert_eq!(source.len(), 1);
        let source = source.pop().unwrap();
        assert_eq!(source.path, tmpdir.path().join("foo/bar/zzz.py"));
    }

    #[test]
    fn test_find_source_file_multiple() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/zzz.py"), "x").unwrap();
        let source = find_source_file(
            tmpdir.path(),
            vec!["foo/*.py"],
            "",
            "",
            None,
            WriteBinTo::None,
        );
        assert_eq!(source.len(), 2);
    }

    #[test]
    fn test_find_source_file_not_found() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/bar")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        let source = find_source_file(
            tmpdir.path(),
            vec!["foo/bar/*.py"],
            "",
            "",
            None,
            WriteBinTo::None,
        );
        assert!(source.is_empty());
    }
}
