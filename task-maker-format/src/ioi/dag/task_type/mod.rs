use std::sync::{Arc, Mutex};

use anyhow::Error;
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

pub use batch::BatchTypeData;
pub use communication::CommunicationTypeData;
use task_maker_dag::FileUuid;

use crate::ioi::{Checker, IOITask, ScoreManager, SubtaskId, TestcaseId};
use crate::{EvaluationData, SourceFile};

mod batch;
mod communication;

/// The type of the task. This changes the behavior of the solutions.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub enum TaskType {
    /// The solution is a single file that will be executed once per testcase, feeding in the input
    /// file and reading the output file. The solution may be compiled with additional graders
    /// (called `grader.LANG`). The output is checked with an external program.
    Batch(BatchTypeData),
    /// The solution is executed in parallel with a manager and communicate using FIFO pipes. There
    /// are only input files since the manager computes the score of the solution.
    Communication(CommunicationTypeData),
    /// Not an actual task.
    None,
}

impl TaskType {
    /// Evaluate a solution on a testcase, eventually adding to the `ScoreManager` the result of the
    /// evaluation. This will add both the execution as well as the checking to the DAG.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate(
        &self,
        task: &IOITask,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        source_file: &SourceFile,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
        correct_output: Option<FileUuid>,
        score_manager: Arc<Mutex<ScoreManager>>,
    ) -> Result<(), Error> {
        match self {
            TaskType::Batch(data) => batch::evaluate(
                task,
                eval,
                subtask_id,
                testcase_id,
                source_file,
                input,
                validation_handle,
                correct_output,
                score_manager,
                data,
            ),
            TaskType::Communication(data) => communication::evaluate(
                task,
                eval,
                subtask_id,
                testcase_id,
                source_file,
                input,
                validation_handle,
                correct_output,
                score_manager,
                data,
            ),
            TaskType::None => Ok(()),
        }
    }

    /// Add to the DAG more executions based on the current task type.
    ///
    /// For example this will force the compilation of the checker in a batch task.
    pub(crate) fn prepare_dag(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        match self {
            TaskType::Batch(batch) => match &batch.checker {
                Checker::Custom(checker) => {
                    checker.prepare(eval)?;
                }
                Checker::WhiteDiff => {}
            },
            TaskType::Communication(communication) => {
                communication.manager.prepare(eval)?;
            }
            TaskType::None => {}
        }
        Ok(())
    }
}
