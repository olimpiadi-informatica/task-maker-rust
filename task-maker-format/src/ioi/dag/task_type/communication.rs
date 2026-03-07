use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Error};
use serde::{Deserialize, Serialize};
use task_maker_dag::{ExecutionGroup, FileUuid, Priority};

use crate::ioi::{Checker, IOITask, ScoreManager, SubtaskId, TestcaseId, EVALUATION_PRIORITY};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, bind_exec_io, EvaluationData, SourceFile, Tag};

/// The type of communication for the solution in a communication task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UserIo {
    /// Communication is achieved by using stdin/stdout.
    StdIo,
    /// Communication is achieved by using the pipes passed in argv.
    FifoIo,
}

impl UserIo {
    /// Used for deserialization.
    /// Returns UserIo::StdIo.
    pub fn std_io() -> Self {
        UserIo::StdIo
    }

    /// Used for deserialization.
    /// Returns UserIo::FifoIo.
    pub fn fifo_io() -> Self {
        UserIo::FifoIo
    }
}

/// The internal data of a task of type `Batch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationTypeData {
    /// The source file of the manager that communicates with the solutions.
    pub manager: Arc<SourceFile>,
    /// Number of solution processes to spawn in parallel in a communication task.
    pub num_processes: u8,
    /// The type of communication for the solution in a communication task.
    pub user_io: UserIo,
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
    _correct_output: Option<FileUuid>,
    score_manager: Arc<Mutex<ScoreManager>>,
    data: &CommunicationTypeData,
) -> Result<(), Error> {
    let mut group = ExecutionGroup::new(format!(
        "Evaluation of {} on testcase {}, subtask {}",
        source_file.name(),
        testcase_id,
        subtask_id
    ));

    let mut fifo_man2sol = Vec::new();
    let mut fifo_sol2man = Vec::new();
    for _ in 0..data.num_processes {
        let fifo1 = group.new_fifo().sandbox_path();
        fifo_man2sol.push(
            fifo1
                .to_str()
                .ok_or_else(|| anyhow!("Non-UTF8 fifo path"))?
                .to_string(),
        );
        let fifo2 = group.new_fifo().sandbox_path();
        fifo_sol2man.push(
            fifo2
                .to_str()
                .ok_or_else(|| anyhow!("Non-UTF8 fifo path"))?
                .to_string(),
        );
    }

    let path = source_file.path.clone();
    let num_processes = data.num_processes as usize;
    for process_index in 0..num_processes {
        let mut args = match data.user_io {
            UserIo::FifoIo => vec![
                fifo_man2sol[process_index].clone(),
                fifo_sol2man[process_index].clone(),
            ],
            UserIo::StdIo => vec![],
        };
        if num_processes > 1 {
            args.push(process_index.to_string());
        }
        let mut sol_exec = source_file
            .execute(
                eval,
                format!(
                    "Evaluation of {} (process {}/{}) on testcase {}, subtask {}",
                    source_file.name(),
                    process_index + 1,
                    num_processes,
                    testcase_id,
                    subtask_id
                ),
                args,
            )
            .context("Failed to execute solution source file")?;
        group.tag = Some(Tag::Evaluation.into());
        group.priority = EVALUATION_PRIORITY - testcase_id as Priority;
        if data.user_io == UserIo::StdIo {
            sol_exec.stdin(task_maker_dag::ExecutionInputBehaviour::Path(
                fifo_man2sol[process_index].clone().into(),
            ));
            sol_exec.stdout_redirect_path(&fifo_sol2man[process_index]);
        }
        let limits = sol_exec.limits_mut();
        if let Some(time_limit) = task.time_limit {
            limits.cpu_time(time_limit);
            limits.wall_time(time_limit * 1.5 + 1.0); // some margin
        }
        if let Some(memory_limit) = task.memory_limit {
            limits.memory(memory_limit * 1024); // MiB -> KiB
        }
        group.add_execution(sol_exec);
    }

    let mut args = Vec::new();
    for process_index in 0..num_processes {
        args.push(&fifo_sol2man[process_index]);
        args.push(&fifo_man2sol[process_index]);
    }
    let mut manager_exec = data
        .manager
        .execute(
            eval,
            format!(
                "Manager of {} on testcase {}, subtask {}",
                source_file.name(),
                testcase_id,
                subtask_id
            ),
            args,
        )
        .context("Failed to execute manager source file")?;
    manager_exec.capture_stdout(Some(128));
    manager_exec.capture_stderr(Some(1024));
    bind_exec_io!(manager_exec, task, input, validation_handle);
    let limits = manager_exec.limits_mut();
    if let Some(time_limit) = task.time_limit {
        let cpu_time = (time_limit + 1.0) * num_processes as f64;
        let wall_time = cpu_time * 1.5 + 1.0; // some margin
        limits.cpu_time(cpu_time);
        limits.wall_time(wall_time);
    }
    if let Some(memory_limit) = task.memory_limit {
        limits.memory(memory_limit * 1024); // MiB -> KiB
    }
    bind_exec_callbacks!(
        eval,
        group.uuid,
        |status, solution| UIMessage::IOIEvaluation {
            subtask: subtask_id,
            testcase: testcase_id,
            solution,
            status,
            manager_index: Some(num_processes)
        },
        path
    )?;
    let sender = eval.sender.clone();
    eval.dag.on_execution_done(&group.uuid, move |results| {
        let send_score = |score, message: String| {
            score_manager
                .lock()
                .unwrap()
                .score(
                    subtask_id,
                    testcase_id,
                    score,
                    message.clone(),
                    sender.clone(),
                )
                .with_context(|| {
                    format!("Failed to store testcase score (score: {score}, message: {message})")
                })
        };
        for (i, result) in results.iter().enumerate() {
            if !result.status.is_success() {
                send_score(
                    0.0,
                    if i == num_processes {
                        "Manager failed".to_string()
                    } else {
                        format!("{:?}", result.status)
                    },
                )?;
                return Ok(());
            }
        }
        let manager_result = &results[num_processes];
        let stdout = manager_result
            .stdout
            .as_ref()
            .ok_or_else(|| anyhow!("Checker stdout not captured"))?;
        let stderr = manager_result
            .stderr
            .as_ref()
            .ok_or_else(|| anyhow!("Checker stderr not captured"))?;
        let score = String::from_utf8_lossy(stdout);
        let score: f64 = score.trim().parse().context("Invalid score from checker")?;
        let message = String::from_utf8_lossy(stderr).trim().to_string();
        let message = Checker::translate_checker_message(message);
        send_score(score, message)
    });
    group.add_execution(manager_exec);
    eval.dag.add_execution_group(group);
    Ok(())
}
