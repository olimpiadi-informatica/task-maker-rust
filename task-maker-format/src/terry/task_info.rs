use anyhow::Error;
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use crate::terry::TerryTask;

/// Task information structure.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TerryTaskInfo {
    /// The version of the `TaskInfo` structure.
    version: u64,
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub description: String,
    /// The maximum score for this task.
    pub max_score: f64,
}

impl TerryTaskInfo {
    /// Generate the task information from the provided `Task`.
    pub fn new(task: &TerryTask) -> Result<TerryTaskInfo, Error> {
        Ok(TerryTaskInfo {
            version: 1,
            name: task.name.clone(),
            description: task.description.clone(),
            max_score: task.max_score,
        })
    }
}
