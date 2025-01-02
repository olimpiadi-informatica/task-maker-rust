use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Error};
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::{Execution, File, FileUuid, Priority};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::{SubtaskId, TestcaseId, GENERATION_PRIORITY, STDERR_CONTENT_LENGTH};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, UISender};
use crate::{EvaluationData, SourceFile, Tag};

/// The file name of the input file that the `InputValidator` has to validate. This file will be
/// placed in the current working directory of the validation sandbox.
pub const TM_VALIDATION_FILE_NAME: &str = "tm_validation_file";

/// An input file validator is responsible for checking that the input file follows the format and
/// constraints defined by the task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify, Default)]
pub enum InputValidator {
    /// Skip the validation and assume the input file is valid.
    #[default]
    AssumeValid,
    /// Use a custom command to check if the input file is valid. The command should exit with
    /// non-zero return code if and only if the input is invalid.
    Custom(Arc<SourceFile>, Vec<String>),
}

impl InputValidator {
    /// Build the execution for the validation of the input file. Return the handle to the standard
    /// output of the validator, if any and the `Execution` if any. The execution does not send UI
    /// messages yet and it's not added to the DAG.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate(
        &self,
        eval: &mut EvaluationData,
        task_path: &Path,
        description: String,
        subtask_id: SubtaskId,
        subtask_name: Option<&str>,
        testcase_id: TestcaseId,
        input: FileUuid,
    ) -> Result<(Option<FileUuid>, Option<Execution>), Error> {
        match self {
            InputValidator::AssumeValid => Ok((None, None)),
            InputValidator::Custom(source_file, args) => {
                let mut exec = source_file
                    .execute(eval, description, args.clone())
                    .context("Failed to execute validator source file")?;
                exec.input(input, TM_VALIDATION_FILE_NAME, false)
                    .tag(Tag::Generation.into())
                    .priority(GENERATION_PRIORITY - testcase_id as Priority)
                    .env("TM_SUBTASK", subtask_id.to_string())
                    .env("TM_TESTCASE", testcase_id.to_string());
                if let Some(name) = subtask_name {
                    exec.env("TM_SUBTASK_NAME", name);
                }
                exec.limits_mut().allow_multiprocess();

                // Add limiti.yaml and constraints.yaml file to the sandbox of the validator
                for filename in &["limiti.yaml", "constraints.yaml"] {
                    let path = task_path.join("gen").join(filename);

                    if !path.is_file() {
                        continue;
                    }

                    let file = File::new(format!("Constraints file at {}", path.display()));
                    exec.input(&file, filename, false);
                    eval.dag.provide_file(file, path)?;
                }

                let stdout = exec.stdout();

                Ok((Some(stdout.uuid), Some(exec)))
            }
        }
    }

    /// Add the validation of the input file to the DAG and the callbacks to the UI, optionally
    /// returning a fake file that blocks the usage of the actual input until the validation
    /// succeeds. If the validation is ignored, `None` is returned.
    pub(crate) fn validate_and_bind(
        &self,
        eval: &mut EvaluationData,
        task_path: &Path,
        subtask_id: SubtaskId,
        subtask_name: Option<&str>,
        testcase_id: TestcaseId,
        input: FileUuid,
    ) -> Result<Option<FileUuid>, Error> {
        let (handle, val) = self.validate(
            eval,
            task_path,
            format!(
                "Validation of input file of testcase {}, subtask {}",
                testcase_id, subtask_id
            ),
            subtask_id,
            subtask_name,
            testcase_id,
            input,
        )?;
        if let Some(mut val) = val {
            val.capture_stderr(STDERR_CONTENT_LENGTH);
            bind_exec_callbacks!(eval, val.uuid, |status| UIMessage::IOIValidation {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            let sender = eval.sender.clone();
            eval.dag.on_execution_done(&val.uuid, move |result| {
                if !result.status.is_success() {
                    let mut diagnostic = Diagnostic::error(format!(
                        "Failed to validate input {} for subtask {}",
                        testcase_id, subtask_id
                    ));
                    if let Some(stderr) = result.stderr {
                        diagnostic = diagnostic.with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            });
            eval.dag.add_execution(val);
        }
        Ok(handle)
    }
}
