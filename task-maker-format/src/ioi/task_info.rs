use std::path::PathBuf;

use failure::Error;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::ioi::Task;

/// Task information structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// Version of this task-info structure.
    version: u64,
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

impl TaskInfo {
    /// Generate the task information from the provided `Task`.
    pub fn new(task: &Task) -> Result<TaskInfo, Error> {
        Ok(TaskInfo {
            version: 1,
            name: task.name.clone(),
            title: task.title.clone(),
            scoring: TaskInfoScoring {
                max_score: task
                    .subtasks
                    .iter()
                    .fold(0.0, |sum, (_, subtask)| sum + subtask.max_score),
                subtasks: task
                    .subtasks
                    .iter()
                    .sorted_by_key(|(&id, _)| id)
                    .map(|(_, subtask)| TaskInfoSubtask {
                        max_score: subtask.max_score,
                        testcases: subtask.testcases.len() as u64,
                    })
                    .collect(),
            },
            limits: TaskInfoLimits {
                time: task.time_limit,
                memory: task.memory_limit,
            },
            statements: task
                .booklets
                .iter()
                .map(|booklet| TaskInfoStatement {
                    language: booklet.config.language.clone(),
                    content_type: mime_guess::from_path(&booklet.dest)
                        .first()
                        .map_or("UNKNOWN".into(), |t| t.to_string()),
                    path: booklet.dest.strip_prefix(&task.path).unwrap().into(),
                })
                .collect(),
            attachments: task
                .path
                .join("att")
                .read_dir()?
                .filter(|entry| entry.as_ref().unwrap().file_type().unwrap().is_file())
                .map(|entry| {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    TaskInfoAttachment {
                        name: entry.file_name().to_str().unwrap().into(),
                        content_type: mime_guess::from_path(path)
                            .first()
                            .map_or("UNKNOWN".into(), |t| t.to_string()),
                        path: entry.path().strip_prefix(&task.path).unwrap().into(),
                    }
                })
                .collect(),
        })
    }
}
