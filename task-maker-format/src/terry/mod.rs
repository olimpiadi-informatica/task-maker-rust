//! The Terry task format.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Serialize};

use crate::terry::dag::{Checker, InputGenerator, InputValidator};
use crate::terry::format::parse_task;
use crate::ui::{UIMessageSender, UIType, UI};
use crate::{EvaluationConfig, EvaluationData, SourceFile, TaskFormat, TaskInfo};

mod dag;
mod format;

/// The type of the seed of a generator for an input file.
pub type Seed = u64;

/// Information about a generic Terry task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Path of the directory of the task.
    pub path: PathBuf,
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub description: String,
    /// The maximum score for this task.
    pub max_score: f64,

    /// The generator of input files of this task.
    pub generator: InputGenerator,
    /// The validator of input files of this task.
    pub validator: Option<InputValidator>,
    /// The checker of input/output files of this task.
    pub checker: Checker,
    /// The official solution of this task, if any. Will be compiled and placed in the sandbox of
    /// the generation/validation/checking.
    pub official_solution: Option<Arc<SourceFile>>,
}

impl Task {
    /// Try to make a `Task` from the specified path. Will return `Err` if the format of the task
    /// is not Terry or if the task is corrupted and cannot be parsed.
    pub fn new<P: AsRef<Path>>(path: P, eval_config: &EvaluationConfig) -> Result<Task, Error> {
        parse_task(path.as_ref(), eval_config)
    }

    /// Check if in the provided path there could be a Terry-like task.
    pub fn is_valid<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().join("task.yaml").exists() && path.as_ref().join("managers").is_dir()
    }
}

impl TaskFormat for Task {
    fn path(&self) -> &Path {
        &self.path
    }

    fn ui(&self, ui_type: &UIType) -> Result<Box<dyn UI>, Error> {
        unimplemented!()
    }

    fn build_dag(&self, eval: &mut EvaluationData, config: &EvaluationConfig) -> Result<(), Error> {
        unimplemented!()
    }

    fn sanity_check_post_hook(&self, ui: &mut UIMessageSender) -> Result<(), Error> {
        unimplemented!()
    }

    fn clean(&self) -> Result<(), Error> {
        unimplemented!()
    }

    fn task_info(&self) -> Result<TaskInfo, Error> {
        unimplemented!()
    }
}
