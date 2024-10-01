use std::path::PathBuf;
use std::fs::FileType;

use anyhow::Error;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use crate::ioi::IOITask;

/// Task information structure.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct IOITaskInfo {
    /// Version of this task-info structure.
    version: u64,
    /// Short name of the task.
    pub name: String,
    /// Title of the task.
    pub title: String,
    /// Scoring info.
    pub scoring: TaskInfoScoring,
    /// Limits of the task.
    pub limits: TaskInfoLimits,
    /// Statements of the task.
    pub statements: Vec<TaskInfoStatement>,
    /// Attachments of the task.
    pub attachments: Vec<TaskInfoAttachment>,
}

/// Limits of the task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TaskInfoLimits {
    /// Time limit in seconds.
    pub time: Option<f64>,
    /// Memory limit in megabytes.
    pub memory: Option<u64>,
}

/// Attachment of the task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TaskInfoAttachment {
    /// Name of this attachment.
    pub name: String,
    /// MIME type of this attachment.
    pub content_type: String,
    /// Path of this attachment relative to task directory.
    pub path: PathBuf,
}

/// Info of the subtasks.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TaskInfoSubtask {
    /// Maximum score for this subtask.
    pub max_score: f64,
    /// Number of testcases for this subtask.
    pub testcases: u64,
}

/// Scoring for the task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TaskInfoScoring {
    /// Maximum score for the task.
    pub max_score: f64,
    /// Subtasks of this task.
    pub subtasks: Vec<TaskInfoSubtask>,
}

/// Statement of the task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct TaskInfoStatement {
    /// Language of the statement.
    pub language: String,
    /// Content type of the statement, as MIME type.
    pub content_type: String,
    /// Path of the task, relative to the task directory.
    pub path: PathBuf,
}

fn is_file_or_symlink(file_type: FileType) -> bool {
    file_type.is_file() || file_type.is_symlink()
}

impl IOITaskInfo {
    /// Generate the task information from the provided `Task`.
    pub fn new(task: &IOITask) -> Result<IOITaskInfo, Error> {
        Ok(IOITaskInfo {
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
                    path: task.path_of(&booklet.dest).into(),
                })
                .collect(),
            attachments: task
                .path
                .join("att")
                .read_dir()
                .map(|dir| {
                    dir.filter(|entry| is_file_or_symlink(entry.as_ref().unwrap().file_type().unwrap()))
                        .map(|entry| {
                            let entry = entry.unwrap();
                            let path = entry.path();
                            TaskInfoAttachment {
                                name: entry.file_name().to_str().unwrap().into(),
                                content_type: mime_guess::from_path(path)
                                    .first()
                                    .map_or("UNKNOWN".into(), |t| t.to_string()),
                                path: task.path_of(&entry.path()).into(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}
