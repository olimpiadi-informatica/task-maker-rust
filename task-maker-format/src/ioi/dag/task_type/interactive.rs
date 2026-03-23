use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Error};
use serde::{Deserialize, Serialize};
use task_maker_dag::{ControllerSettings, ExecutionGroup, FileUuid, Priority};

use crate::ioi::{Checker, IOITask, ScoreManager, SubtaskId, TestcaseId, EVALUATION_PRIORITY};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, EvaluationData, SourceFile, Tag};

/// The internal data of a task of type `Interactive`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveTypeData {
    /// The source file of the controller.
    pub controller: Arc<SourceFile>,
    /// Time limit for the controller.
    pub controller_time_limit: Option<f64>,
    /// Wall time limit for the controller.
    pub controller_wall_time_limit: Option<f64>,
    /// Memory limit in MiB for the controller.
    pub controller_memory_limit: Option<u64>,
    /// Upper bound on the number of processes the controller can spawn.
    pub controller_process_limit: Option<usize>,
    /// Whether the solution processes are assumed to be concurrent (wall time max-ed, memory summed)
    /// or sequential (wall time sum-ed, memory max-ed).
    pub concurrent: Option<bool>,
}

/// Evaluate a solution in a task of Interactive type.
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
    data: &InteractiveTypeData,
) -> Result<(), Error> {
    let mut group = ExecutionGroup::new(format!(
        "Evaluation of {} on testcase {}, subtask {}",
        source_file.name(),
        testcase_id,
        subtask_id
    ));
    group.tag = Some(Tag::Evaluation.into());
    group.priority = EVALUATION_PRIORITY - testcase_id as Priority;

    group.controller_settings = Some(ControllerSettings {
        process_limit: data.controller_process_limit.unwrap_or(200),
        concurrent: data.concurrent.unwrap_or(true),
    });

    let mut controller_exec = data
        .controller
        .execute(
            eval,
            format!(
                "Controller of {} on testcase {}, subtask {}",
                source_file.name(),
                testcase_id,
                subtask_id
            ),
            Vec::<String>::new(),
        )
        .context("Failed to execute controller source file")?;

    let infile_name = if let Some(infile) = &task.infile {
        infile.clone()
    } else {
        "input.txt".into()
    };
    controller_exec.input(input, infile_name, false);

    if let Some(file) = validation_handle {
        controller_exec.input(file, "wait_for_validation", false);
    }

    controller_exec.stdin = task_maker_dag::ExecutionInputBehaviour::Ignored;
    controller_exec.stdout = task_maker_dag::ExecutionOutputBehaviour::Ignored;
    controller_exec.capture_stderr(Some(1024 * 1024));

    let limits = controller_exec.limits_mut();
    if let Some(time_limit) = data.controller_time_limit {
        limits.cpu_time(time_limit);
    }
    if let Some(wall_time) = data.controller_wall_time_limit {
        limits.wall_time(wall_time);
    }
    if let Some(memory_limit) = data.controller_memory_limit {
        limits.memory(memory_limit * 1024); // MiB -> KiB
    }
    group.add_execution(controller_exec);

    let mut sol_exec = source_file
        .execute(
            eval,
            format!(
                "Solution prefix of {} on testcase {}, subtask {}",
                source_file.name(),
                testcase_id,
                subtask_id
            ),
            Vec::<String>::new(),
        )
        .context("Failed to execute solution source file")?;

    sol_exec.stdin = task_maker_dag::ExecutionInputBehaviour::Inherit;
    sol_exec.stdout = task_maker_dag::ExecutionOutputBehaviour::Inherit;
    sol_exec.stderr = task_maker_dag::ExecutionOutputBehaviour::Ignored;

    let sol_limits = sol_exec.limits_mut();
    if let Some(time_limit) = task.time_limit {
        sol_limits.cpu_time(time_limit);
        sol_limits.wall_time(time_limit * 1.5 + 1.0); // some margin
    }
    if let Some(memory_limit) = task.memory_limit {
        sol_limits.memory(memory_limit * 1024); // MiB -> KiB
    }
    group.add_execution(sol_exec);

    let path = source_file.path.clone();
    bind_exec_callbacks!(
        eval,
        group.uuid,
        |status, solution| UIMessage::IOIEvaluation {
            subtask: subtask_id,
            testcase: testcase_id,
            solution,
            status,
            manager_index: Some(0)
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

        let controller_result = &results[0];
        if !controller_result.status.is_success() {
            return send_score(
                0.0,
                format!("Controller died ({:?})", controller_result.status),
            );
        }

        let stderr = controller_result
            .stderr
            .as_ref()
            .ok_or_else(|| anyhow!("Controller stderr not captured"))?;
        let stderr_str = String::from_utf8_lossy(stderr);

        let mut score: Option<f64> = None;
        let mut message = String::new();
        let mut admin_message = String::new();

        for line in stderr_str.lines() {
            if let Some(stripped) = line.strip_prefix("SCORE: ") {
                score = stripped.trim().parse().ok();
            } else if let Some(stripped) = line.strip_prefix("USER_MESSAGE: ") {
                message = stripped.trim().to_string();
            } else if let Some(stripped) = line.strip_prefix("ADMIN_MESSAGE: ") {
                admin_message = stripped.trim().to_string();
            }
        }

        let score = score.unwrap_or(0.0);
        let mut message = Checker::translate_checker_message(message);
        if !admin_message.is_empty() {
            message = format!("{message} (Admin-only message: {admin_message})");
        }
        send_score(score, message)
    });

    eval.dag.add_execution_group(group);
    Ok(())
}
