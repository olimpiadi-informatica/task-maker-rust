use std::path::Path;

use anyhow::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::ExecutionDAGConfig;

use crate::{ui, EvaluationConfig, EvaluationData, IOITask, TaskInfo, TerryTask, UI};

/// The format of the task.
/// A task format, providing a UI and the parsing and execution abilities.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskFormat {
    /// The task is IOI-like.
    IOI(IOITask),
    /// The task is Terry-like.
    Terry(TerryTask),
}

impl TaskFormat {
    /// Get the root directory of the task.
    pub fn path(&self) -> &Path {
        match self {
            TaskFormat::IOI(task) => task.path(),
            TaskFormat::Terry(task) => task.path(),
        }
    }

    /// Get an appropriate `UI` for this task.
    pub fn ui(
        &self,
        ui_type: &ui::UIType,
        config: ExecutionDAGConfig,
    ) -> Result<Box<dyn UI>, Error> {
        match self {
            TaskFormat::IOI(task) => task.ui(ui_type, config),
            TaskFormat::Terry(task) => task.ui(ui_type, config),
        }
    }

    /// Add the executions required for evaluating this task to the execution DAG.
    pub fn build_dag(
        &mut self,
        eval: &mut EvaluationData,
        config: &EvaluationConfig,
    ) -> Result<(), Error> {
        match self {
            TaskFormat::IOI(task) => task.build_dag(eval, config),
            TaskFormat::Terry(task) => task.build_dag(eval, config),
        }
    }

    /// Hook called after the execution completed, useful for sending messages to the UI about the
    /// results of the sanity checks with data available only after the evaluation.
    pub fn sanity_check_post_hook(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        match self {
            TaskFormat::IOI(task) => task.sanity_check_post_hook(eval),
            TaskFormat::Terry(task) => task.sanity_check_post_hook(eval),
        }
    }

    /// Clean the task folder removing the files that can be generated automatically.
    pub fn clean(&self) -> Result<(), Error> {
        match self {
            TaskFormat::IOI(task) => task.clean(),
            TaskFormat::Terry(task) => task.clean(),
        }
    }

    /// Get the task information.
    pub fn task_info(&self) -> Result<TaskInfo, Error> {
        match self {
            TaskFormat::IOI(task) => task.task_info(),
            TaskFormat::Terry(task) => task.task_info(),
        }
    }
}

impl From<IOITask> for TaskFormat {
    fn from(task: IOITask) -> Self {
        Self::IOI(task)
    }
}

impl From<TerryTask> for TaskFormat {
    fn from(task: TerryTask) -> Self {
        Self::Terry(task)
    }
}
