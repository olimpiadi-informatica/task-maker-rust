use std::sync::{Arc, Mutex};

use failure::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::{ExecutionStatus, FileUuid};

use crate::ioi::{ScoreManager, SubtaskId, Tag, Task, TestcaseId};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, bind_exec_io};
use crate::{EvaluationData, SourceFile};

/// The type of the task. This changes the behavior of the solutions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// The solution is a single file that will be executed once per testcase, feeding in the input
    /// file and reading the output file. The solution may be compiled with additional graders
    /// (called `grader.LANG`). The output is checked with an external program.
    Batch,
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
        correct_output: FileUuid,
        score_manager: Arc<Mutex<ScoreManager>>,
    ) -> Result<(), Error> {
        match self {
            TaskType::Batch => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Evaluation of {} on testcase {}, subtask {}",
                        source_file.name(),
                        testcase_id,
                        subtask_id
                    ),
                    Vec::<String>::new(),
                )?;
                exec.tag(Tag::Evaluation.into());
                let output = bind_exec_io!(exec, task, input, validation_handle);
                let path = source_file.path.clone();
                let limits = exec.limits_mut();
                if let Some(time_limit) = task.time_limit {
                    limits.cpu_time(time_limit);
                    limits.wall_time(time_limit * 1.5 + 1.0); // some margin
                }
                if let Some(memory_limit) = task.memory_limit {
                    limits.memory(memory_limit * 1024); // MiB -> KiB
                }
                bind_exec_callbacks!(
                    eval,
                    exec.uuid,
                    |status, solution| UIMessage::IOIEvaluation {
                        subtask: subtask_id,
                        testcase: testcase_id,
                        solution,
                        status
                    },
                    path
                )?;
                let sender = eval.sender.clone();
                let path = source_file.path.clone();
                let score_manager_err = score_manager.clone();
                eval.dag
                    .on_execution_done(&exec.uuid, move |result| match result.status {
                        ExecutionStatus::Success => Ok(()),
                        _ => score_manager_err.lock().unwrap().score(
                            subtask_id,
                            testcase_id,
                            0.0,
                            format!("{:?}", result.status),
                            sender,
                            path,
                        ),
                    });
                eval.dag.add_execution(exec);

                let sender = eval.sender.clone();
                let path = source_file.path.clone();
                task.checker.check_and_bind(
                    eval,
                    subtask_id,
                    testcase_id,
                    source_file.path.clone(),
                    input,
                    correct_output,
                    output.uuid,
                    move |score, message| {
                        score_manager.lock().unwrap().score(
                            subtask_id,
                            testcase_id,
                            score,
                            message,
                            sender,
                            path,
                        )
                    },
                )?;
            }
        };
        Ok(())
    }
}
