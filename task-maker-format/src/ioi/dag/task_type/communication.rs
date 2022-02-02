use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Error};
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::{ExecutionGroup, FileUuid, Priority};

use crate::ioi::{Checker, IOITask, ScoreManager, SubtaskId, TestcaseId, EVALUATION_PRIORITY};
use crate::ui::{UIMessage, UIMessageSender};
use crate::{bind_exec_callbacks, bind_exec_io};
use crate::{EvaluationData, SourceFile, Tag};

/// The type of communication for the solution in a communication task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TypeScriptify, Eq, PartialEq)]
pub enum UserIo {
    /// Communication is achieved by using stdin/stdout.
    StdIo,
    /// Communication is achieved by using the pipes passed in argv.
    FifoIo,
}

/// The internal data of a task of type `Batch`.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct CommunicationTypeData {
    /// The source file of the manager that communicates with the solutions.
    pub manager: Arc<SourceFile>,
    /// Number of solution processes to spawn in parallel in a communication task.
    pub num_processes: u8,
    /// The type of communication for the solution in a communication task.
    pub user_io: UserIo,
}

/// Internal data of `ScoreSender`.
#[derive(Debug, Clone)]
struct ScoreSenderData {
    /// The id of the current subtask.
    subtask_id: SubtaskId,
    /// The id of the current testcase.
    testcase_id: TestcaseId,
    /// The sender to use for with the `ScoreManager`.
    sender: Arc<Mutex<UIMessageSender>>,
    /// The path of the current solution.
    path: PathBuf,
    /// The score manager to use for sending the score.
    score_manager: Arc<Mutex<ScoreManager>>,
    /// Whether the score has already been sent. This avoids sending the score more than once.
    done: bool,
}

/// Utility structure for sending the score only once. Since there are many points where the score
/// can be generated (the manager, but also each process if it fails), it's easier to centralize the
/// control of the sending.
/// It's important that, in case of a failure, the first process that fails is marked as the cause
/// because it will stop the entire group, maybe letting the other executions fail.
#[derive(Debug, Clone)]
struct ScoreSender {
    /// Interior mutability allowing this struct to be Clone, Send and Sync.
    data: Arc<Mutex<ScoreSenderData>>,
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
    let score_sender = ScoreSender::new(
        subtask_id,
        testcase_id,
        eval.sender.clone(),
        path.clone(),
        score_manager,
    );
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
        let mut exec = source_file
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
        if data.user_io == UserIo::StdIo {
            exec.stdin_redirect_path(&fifo_man2sol[process_index]);
            exec.stdout_redirect_path(&fifo_sol2man[process_index]);
        }
        exec.tag(Tag::Evaluation.into());
        exec.priority(EVALUATION_PRIORITY - testcase_id as Priority);
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
                status,
                part: process_index,
                num_parts: num_processes,
            },
            path
        )?;
        let score_sender = score_sender.clone();
        eval.dag.on_execution_done(&exec.uuid, move |result| {
            if !result.status.is_success() {
                score_sender.send(0.0, format!("{:?}", result.status))?;
            }
            Ok(())
        });
        group.add_execution(exec);
    }

    let mut args = Vec::new();
    for process_index in 0..num_processes {
        args.push(&fifo_sol2man[process_index]);
        args.push(&fifo_man2sol[process_index]);
    }
    let mut exec = data
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
    exec.tag(Tag::Evaluation.into())
        .priority(EVALUATION_PRIORITY - testcase_id as Priority)
        .capture_stdout(128)
        .capture_stderr(1024);
    bind_exec_io!(exec, task, input, validation_handle);
    let limits = exec.limits_mut();
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
        exec.uuid,
        |status, solution| UIMessage::IOIChecker {
            subtask: subtask_id,
            testcase: testcase_id,
            solution,
            status,
        },
        path
    )?;
    eval.dag.on_execution_done(&exec.uuid, move |result| {
        if !result.status.is_success() {
            score_sender.send(0.0, "Checker failed".to_string())?;
            return Ok(());
        }
        let stdout = result
            .stdout
            .ok_or_else(|| anyhow!("Checker stdout not captured"))?;
        let stderr = result
            .stderr
            .ok_or_else(|| anyhow!("Checker stderr not captured"))?;
        let score = String::from_utf8_lossy(&stdout);
        let score: f64 = score.trim().parse().context("Invalid score from checker")?;
        let message = String::from_utf8_lossy(&stderr).trim().to_string();
        let message = Checker::translate_checker_message(message);
        score_sender.send(score, message)?;
        Ok(())
    });
    group.add_execution(exec);
    eval.dag.add_execution_group(group);
    Ok(())
}

impl ScoreSender {
    /// Make a new `ScoreSender` for a testcase of a solution.
    fn new(
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        sender: Arc<Mutex<UIMessageSender>>,
        path: PathBuf,
        score_manager: Arc<Mutex<ScoreManager>>,
    ) -> ScoreSender {
        ScoreSender {
            data: Arc::new(Mutex::new(ScoreSenderData {
                subtask_id,
                testcase_id,
                sender,
                path,
                score_manager,
                done: false,
            })),
        }
    }

    /// Send the score to the `ScoreManager`, if not already sent.
    fn send(&self, score: f64, message: String) -> Result<(), Error> {
        let data = self.data.lock().unwrap();
        // do not send the score twice
        if data.done {
            return Ok(());
        }
        data.score_manager
            .lock()
            .unwrap()
            .score(
                data.subtask_id,
                data.testcase_id,
                score,
                message.clone(),
                data.sender.clone(),
                data.path.clone(),
            )
            .with_context(|| {
                format!(
                    "Failed to store testcase score (score: {}, message: {})",
                    score, message
                )
            })?;
        Ok(())
    }
}
