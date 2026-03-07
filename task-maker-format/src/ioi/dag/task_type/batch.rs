use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Error};
use serde::{Deserialize, Serialize};
use task_maker_dag::{ExecutionStatus, FileUuid, Priority};

use crate::ioi::{
    Checker, IOITask, OutputGenerator, ScoreManager, SubtaskId, TestcaseId, EVALUATION_PRIORITY,
};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, bind_exec_io, EvaluationData, SourceFile, Tag};

/// The internal data of a task of type `Batch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTypeData {
    /// The default output generator for this task, if any.
    #[serde(skip_serializing)]
    pub output_generator: Option<OutputGenerator>,
    /// The checker to use for this task.
    pub checker: Checker,
}

/// Evaluate a solution in a task of Batch type.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    task: &IOITask,
    eval: &mut EvaluationData,
    subtask_id: SubtaskId,
    testcase_id: TestcaseId,
    source_file: &SourceFile,
    input: FileUuid,
    validation_handle: Option<FileUuid>,
    correct_output: Option<FileUuid>,
    score_manager: Arc<Mutex<ScoreManager>>,
    data: &BatchTypeData,
) -> Result<(), Error> {
    let correct_output = correct_output.ok_or_else(|| anyhow!("Missing official solution"))?;
    let mut exec = source_file
        .execute(
            eval,
            format!(
                "Evaluation of {} on testcase {}, subtask {}",
                source_file.name(),
                testcase_id,
                subtask_id
            ),
            Vec::<String>::new(),
        )
        .context("Failed to execute solution source file")?;
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
    let mut group = exec.into_group();
    group.tag = Some(Tag::Evaluation.into());
    group.priority = EVALUATION_PRIORITY - testcase_id as Priority;
    bind_exec_callbacks!(
        eval,
        group.uuid,
        |status, solution| UIMessage::IOIEvaluation {
            subtask: subtask_id,
            testcase: testcase_id,
            solution,
            status,
            manager_index: None
        },
        path
    )?;
    let sender = eval.sender.clone();
    let score_manager_err = score_manager.clone();
    eval.dag.on_execution_done(&group.uuid, move |results| {
        assert_eq!(results.len(), 1);
        let result = &results[0];
        match result.status {
            ExecutionStatus::Success => Ok(()),
            _ => score_manager_err.lock().unwrap().score(
                subtask_id,
                testcase_id,
                0.0,
                format!("{:?}", result.status),
                sender,
            ),
        }
    });
    eval.dag.add_execution_group(group);

    let sender = eval.sender.clone();
    data.checker.check_and_bind(
        eval,
        subtask_id,
        testcase_id,
        source_file.path.clone(),
        input,
        correct_output,
        output.uuid,
        move |score, message| {
            score_manager
                .lock()
                .unwrap()
                .score(subtask_id, testcase_id, score, message, sender)
        },
    )?;
    Ok(())
}
