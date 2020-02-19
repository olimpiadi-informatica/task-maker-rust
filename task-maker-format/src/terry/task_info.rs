use failure::Error;
use serde::{Deserialize, Serialize};

use crate::terry::Task;

/// Task information structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// The version of the `TaskInfo` structure.
    version: u64,
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub description: String,
    /// The maximum score for this task.
    pub max_score: f64,
}

impl TaskInfo {
    /// Generate the task information from the provided `Task`.
    pub fn new(task: &Task) -> Result<TaskInfo, Error> {
        Ok(TaskInfo {
            version: 1,
            name: task.name.clone(),
            description: task.description.clone(),
            max_score: task.max_score,
        })
    }
}
