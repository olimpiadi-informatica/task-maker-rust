use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Error};
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::{Execution, ExecutionCommand, ExecutionStatus, FileUuid, Priority};

use crate::bind_exec_callbacks;
use crate::ioi::{SubtaskId, TestcaseId, EVALUATION_PRIORITY};
use crate::ui::UIMessage;
use crate::{EvaluationData, SourceFile, Tag};

/// Which tool to use to compute the score on a testcase given the input file, the _correct_ output
/// file and the output file to evaluate.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub enum Checker {
    /// Use a built-in white diff checker that scores 1.0 if the two output files are identical
    /// except for white spaces. It internally uses `diff --ignore-all-spaces`
    WhiteDiff,
    /// Use a custom checker based on an executable that can output a score (from 0.0 to 1.0) to
    /// stdout as well as a custom message on stderr.
    ///
    /// The arguments are the paths of (input, correct_output, test_output). The checker should
    /// output to stdout the score and to stderr a message for the user.
    Custom(Arc<SourceFile>),
}

impl Checker {
    /// Build the execution of the checker for the specified files, the callback will be called when
    /// the result is ready. The execution does not send UI messages yet and it's not added to the
    /// DAG.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check<F>(
        &self,
        eval: &mut EvaluationData,
        testcase_id: TestcaseId,
        description: String,
        input: FileUuid,
        correct_output: FileUuid,
        test_output: FileUuid,
        callback: F,
    ) -> Result<Execution, Error>
    where
        F: FnOnce(f64, String) -> Result<(), Error> + Send + Sync + 'static,
    {
        match self {
            Checker::WhiteDiff => {
                let mut exec = Execution::new(description, ExecutionCommand::system("diff"));
                exec.args(vec![
                    "--ignore-blank-lines",
                    "--ignore-space-change",
                    "correct",
                    "test",
                ])
                .input(correct_output, "correct", false)
                .input(test_output, "test", false)
                .tag(Tag::Checking.into())
                .priority(EVALUATION_PRIORITY - testcase_id as Priority);

                eval.dag.on_execution_done(&exec.uuid, move |result| {
                    match result.status {
                        // diff exits with 0 if the files are equal
                        ExecutionStatus::Success => callback(1.0, "Output is correct".into())
                            .context("Checker callback failed")?,
                        // return code 1 means the files are different
                        ExecutionStatus::ReturnCode(1) => {
                            callback(0.0, "Output is incorrect".into())
                                .context("Checker callback failed")?
                        }
                        _ => unreachable!("diff died badly? {:?}", result),
                    };
                    Ok(())
                });
                Ok(exec)
            }
            Checker::Custom(source_file) => {
                let mut exec = source_file
                    .execute(
                        eval,
                        description,
                        vec!["input", "correct_output", "test_output"],
                    )
                    .context("Failed to execute checker source file")?;
                exec.input(input, "input", false)
                    .input(correct_output, "correct_output", false)
                    .input(test_output, "test_output", false)
                    .tag(Tag::Checking.into())
                    .capture_stdout(128)
                    .capture_stderr(1024)
                    .priority(EVALUATION_PRIORITY - testcase_id as Priority);
                eval.dag.on_execution_done(&exec.uuid, move |res| {
                    let stdout = res
                        .stdout
                        .ok_or_else(|| anyhow!("Checker stdout not captured"))?;
                    let stderr = res
                        .stderr
                        .ok_or_else(|| anyhow!("Checker stderr not captured"))?;
                    let message = String::from_utf8_lossy(&stderr).trim().to_string();
                    let message = Self::translate_checker_message(message);
                    if !res.status.is_success() {
                        bail!(
                            "Checker failed exiting with {:?}, stderr: {}",
                            res.status,
                            message
                        );
                    }
                    let score = String::from_utf8_lossy(&stdout);
                    let score: f64 = score.trim().parse().with_context(|| {
                        format!(
                            "Invalid score {:?} from checker (stderr: {})",
                            score, message
                        )
                    })?;
                    callback(score, message)
                });
                Ok(exec)
            }
        }
    }

    /// Add the checking of the output file to the DAG, binding the callbacks for sending to the UI
    /// the messages as well as calling `callback` with the outcome of the checker.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_and_bind<S: Into<PathBuf>, F>(
        &self,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        solution: S,
        input: FileUuid,
        correct_output: FileUuid,
        test_output: FileUuid,
        callback: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(f64, String) -> Result<(), Error> + Send + Sync + 'static,
    {
        let solution = solution.into();
        let exec = self.check(
            eval,
            testcase_id,
            format!(
                "Checking output of {:?} of testcase {}, subtask {}",
                solution.file_name().unwrap(),
                testcase_id,
                subtask_id
            ),
            input,
            correct_output,
            test_output,
            callback,
        )?;
        bind_exec_callbacks!(
            eval,
            exec.uuid,
            |status, solution| UIMessage::IOIChecker {
                subtask: subtask_id,
                testcase: testcase_id,
                solution,
                status
            },
            solution
        )?;
        eval.dag.add_execution(exec);
        Ok(())
    }

    /// The checker may return a message to be translated. This function maps the message
    /// placeholders to actual messages.
    pub fn translate_checker_message(message: String) -> String {
        match message.as_str() {
            "translate:success" => "Output is correct".into(),
            "translate:partial" => "Output is partially correct".into(),
            "translate:wrong" => "Output is incorrect".into(),
            _ => message,
        }
    }
}
