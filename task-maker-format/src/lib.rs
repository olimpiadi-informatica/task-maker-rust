//! Task parsing and execution using computation DAGs.
//!
//! This crate allows you to parse the tasks on disk and evaluate the solutions inside of them by
//! adding the executions inside an [`ExecutionDAG`](task_maker_dag/struct.ExecutionDAG.html).
//!
//! This crate also provides ui functionalities for showing the progress and the results of the
//! execution.

#![deny(missing_docs)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate approx;

use crate::ui::UI;

pub mod ioi;
mod source_file;
pub mod ui;
pub use source_file::SourceFile;

use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use task_maker_dag::ExecutionDAG;
use task_maker_lang::{GraderMap, LanguageManager};

/// Trait that defines the capabilities of a task format, providing a UI and the parsing and
/// execution abilities.
pub trait TaskFormat {
    /// Get an appropriate `UI` for this task.
    fn ui(&self, ui_type: &ui::UIType) -> Result<Box<dyn UI>, Error>;

    /// Execute the evaluation of this task by adding the executions to the provided DAG.
    fn execute(&self, eval: &mut EvaluationData, config: &EvaluationConfig) -> Result<(), Error>;

    /// Hook called after the execution completed, useful for sending messages to the UI about the
    /// results of the sanity checks with data available only after the evaluation.
    fn sanity_check_post_hook(&self, ui: &mut ui::UIMessageSender) -> Result<(), Error>;

    /// Clean the task folder removing the files that can be generated automatically.
    fn clean(&self) -> Result<(), Error>;

    /// Get task information
    fn task_info(&self) -> Result<TaskInfo, Error>;
}

/// Limits of the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoLimits {
    /// Time limit in seconds.
    time: Option<f64>,
    /// Memory limit in megabytes.
    memory: Option<u64>,
}

/// Attachment of the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoAttachment {
    /// Name of this attachment.
    name: String,
    /// MIME type of this attachment.
    content_type: String,
    /// Path of this attachment relative to task directory.
    path: PathBuf,
}

/// Info of the subtasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoSubtask {
    /// Maximum score for this subtask.
    max_score: f64,
    /// Number of testcases for this subtask.
    testcases: u64,
}

/// Scoring for the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoScoring {
    /// Maximum score for the task.
    max_score: f64,
    /// Subtasks of this task.
    subtasks: Vec<TaskInfoSubtask>,
}

/// Statement of the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoStatement {
    /// Language of the statement.
    language: String,
    /// Content type of the statement, as MIME type.
    content_type: String,
    /// Path of the task, relative to the task directory.
    path: PathBuf,
}

/// Task information structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// Version of this task-info structure.
    version: f32,
    /// Type of the task.
    task_type: String,
    /// Short name of the task.
    name: String,
    /// Title of the task.
    title: String,
    /// Scoring info.
    scoring: TaskInfoScoring,
    /// Limits of the task.
    limits: TaskInfoLimits,
    /// Statements of the task.
    statements: Vec<TaskInfoStatement>,
    /// Attachments of the task.
    attachments: Vec<TaskInfoAttachment>,
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
}

/// The data for an evaluation, including the DAG and the UI channel.
pub struct EvaluationData {
    /// The DAG with the evaluation data.
    pub dag: ExecutionDAG,
    /// The sender of the UI.
    pub sender: Arc<Mutex<ui::UIMessageSender>>,
}

impl EvaluationData {
    /// Crate a new `EvaluationData` returning the data and the receiving part of the UI channel.
    pub fn new() -> (EvaluationData, ui::UIChannelReceiver) {
        let (sender, receiver) = ui::UIMessageSender::new();
        (
            EvaluationData {
                dag: ExecutionDAG::new(),
                sender: Arc::new(Mutex::new(sender)),
            },
            receiver,
        )
    }
}

/// What can send [`UIMessage`](ui/enum.UIMessage.html)s.
pub trait UISender {
    /// Send that `UIMessage` to the UI.
    fn send(&self, message: ui::UIMessage) -> Result<(), Error>;
}

/// Implement `.send(message)` for `Mutex<UIMessageSender>` in order to do
/// `EvaluationData.sender.send(message)`. This will lock the mutex and send the message to the UI.
impl UISender for Mutex<ui::UIMessageSender> {
    fn send(&self, message: ui::UIMessage) -> Result<(), Error> {
        self.lock().unwrap().send(message)
    }
}

/// List all the files inside `cwd` that matches a list of glob patterns. The results are in the
/// same order of the patterns.
pub(crate) fn list_files<P: AsRef<Path>, S: AsRef<str>>(cwd: P, patterns: Vec<S>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for pattern in patterns.into_iter() {
        let pattern = cwd.as_ref().join(pattern.as_ref());
        for file in glob::glob(&pattern.to_string_lossy()).expect("Invalid pattern for list_files")
        {
            results.push(file.unwrap().to_owned());
        }
    }
    results
}

/// Make a `SourceFile` with the first file that matches the patterns provided that is in a
/// recognised language. Returns `None` if no valid source file can be found.
pub(crate) fn find_source_file<
    P: AsRef<Path>,
    S: AsRef<str>,
    P2: Into<PathBuf>,
    P3: Into<PathBuf>,
>(
    cwd: P,
    patterns: Vec<S>,
    base_path: P3,
    grader_map: Option<Arc<GraderMap>>,
    write_bin_to: Option<P2>,
) -> Option<SourceFile> {
    for path in list_files(cwd, patterns) {
        if LanguageManager::detect_language(&path).is_some() {
            // SourceFile::new may fail if the language is unknown
            return Some(SourceFile::new(&path, base_path, grader_map, write_bin_to).unwrap());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_files() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
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
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/bar")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        std::fs::write(tmpdir.path().join("foo/bar/zzz.py"), "x").unwrap();
        let source = find_source_file(
            tmpdir.path(),
            vec!["foo/bar/*.py"],
            "",
            None,
            None::<PathBuf>,
        );
        assert!(source.is_some());
        let source = source.unwrap();
        assert_eq!(source.path, tmpdir.path().join("foo/bar/zzz.py"));
    }

    #[test]
    fn test_find_source_file_not_found() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        std::fs::create_dir_all(tmpdir.path().join("foo/bar")).unwrap();
        std::fs::write(tmpdir.path().join("foo/xxx.py"), "x").unwrap();
        let source = find_source_file(
            tmpdir.path(),
            vec!["foo/bar/*.py"],
            "",
            None,
            None::<PathBuf>,
        );
        assert!(source.is_none());
    }
}
