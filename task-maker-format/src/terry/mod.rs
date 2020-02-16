//! The Terry task format.
use std::path::{Path, PathBuf};

use failure::Error;
use serde::{Deserialize, Serialize};

use crate::ui::{UIMessageSender, UIType, UI};
use crate::{EvaluationConfig, EvaluationData, TaskFormat, TaskInfo};

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
}

impl Task {
    /// Try to make a `Task` from the specified path. Will return `Err` if the format of the task
    /// is not Terry or if the task is corrupted and cannot be parsed.
    pub fn new<P: AsRef<Path>>(path: P, eval_config: &EvaluationConfig) -> Result<Task, Error> {
        unimplemented!();
    }

    /// Check if in the provided path there could be a Terry-like task.
    pub fn is_valid<P: AsRef<Path>>(path: P) -> bool {
        return path.as_ref().join("task.yaml").exists() && path.as_ref().join("managers").is_dir();
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
