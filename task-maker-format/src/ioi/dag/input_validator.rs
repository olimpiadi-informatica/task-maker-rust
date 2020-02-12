use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, FileUuid};

use crate::bind_exec_callbacks;
use crate::ioi::{SubtaskId, Tag, TestcaseId, STDERR_CONTENT_LENGTH};
use crate::ui::UIMessage;
use crate::{EvaluationData, SourceFile, UISender};

/// An input file validator is responsible for checking that the input file follows the format and
/// constraints defined by the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputValidator {
    /// Skip the validation and assume the input file is valid.
    AssumeValid,
    /// Use a custom command to check if the input file is valid. The command should exit with
    /// non-zero return code if and only if the input is invalid.
    Custom(Arc<SourceFile>, Vec<String>),
}

impl InputValidator {
    /// Build the execution for the validation of the input file. Return the handle to the standard
    /// output of the validator, if any and the `Execution` if any. The execution does not send UI
    /// messages yet and it's not added to the DAG.
    pub(crate) fn validate(
        &self,
        eval: &mut EvaluationData,
        description: String,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
    ) -> Result<(Option<FileUuid>, Option<Execution>), Error> {
        match self {
            InputValidator::AssumeValid => Ok((None, None)),
            InputValidator::Custom(source_file, args) => {
                let mut exec = source_file.execute(eval, description, args.clone())?;
                exec.input(input, "tm_validation_file", false)
                    .tag(Tag::Generation.into())
                    .env("TM_SUBTASK", subtask_id.to_string())
                    .env("TM_TESTCASE", testcase_id.to_string());
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
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
    ) -> Result<Option<FileUuid>, Error> {
        let (handle, val) = self.validate(
            eval,
            format!(
                "Validation of input file of testcase {}, subtask {}",
                testcase_id, subtask_id
            ),
            subtask_id,
            testcase_id,
            input,
        )?;
        if let Some(mut val) = val {
            bind_exec_callbacks!(eval, val.uuid, |status| UIMessage::IOIValidation {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            let sender = eval.sender.clone();
            eval.dag
                .get_file_content(val.stderr(), STDERR_CONTENT_LENGTH, move |content| {
                    let content = String::from_utf8_lossy(&content);
                    sender.send(UIMessage::IOIValidationStderr {
                        testcase: testcase_id,
                        subtask: subtask_id,
                        content: content.into(),
                    })
                });
            eval.dag.add_execution(val);
        }
        Ok(handle)
    }
}
