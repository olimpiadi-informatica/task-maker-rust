use std::sync::{Arc, Mutex};

use failure::Error;
use serde::{Deserialize, Serialize};

pub use batch::BatchTypeData;
pub use communication::CommunicationTypeData;
use task_maker_dag::FileUuid;

use crate::ioi::{ScoreManager, SubtaskId, Task, TestcaseId};
use crate::{EvaluationData, SourceFile};

mod batch;
mod communication;

/// The type of the task. This changes the behavior of the solutions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// The solution is a single file that will be executed once per testcase, feeding in the input
    /// file and reading the output file. The solution may be compiled with additional graders
    /// (called `grader.LANG`). The output is checked with an external program.
    Batch(BatchTypeData),
    /// The solution is executed in parallel with a manager and communicate using FIFO pipes. There
    /// are only input files since the manager computes the score of the solution.
    Communication(CommunicationTypeData),
}

impl TaskType {
    /// Evaluate a solution on a testcase, eventually adding to the `ScoreManager` the result of the
    /// evaluation. This will add both the execution as well as the checking to the DAG.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate(
        &self,
        task: &Task,
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
        }
    }
}
