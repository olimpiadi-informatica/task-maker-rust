use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Error};
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, File, FileUuid, Priority};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::{IOITask, SubtaskId, TestcaseId, GENERATION_PRIORITY, STDERR_CONTENT_LENGTH};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, bind_exec_io, UISender};
use crate::{EvaluationData, SourceFile, Tag};

/// The source of the output files. It can either be a statically provided output file or a custom
/// command that will generate an output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputGenerator {
    /// The output generator is not available.
    NotAvailable,
    /// Use the static file as output. The file will be copied without transformations.
    StaticFile(PathBuf),
    /// Use a custom command to generate the output file. The task specification for input/output
    /// files are used.
    Custom(Arc<SourceFile>, Vec<String>),
}

impl OutputGenerator {
    /// Build the execution for the generation of the output file. Return the handle to the output
    /// file and the `Execution` if any. The execution does not send UI messages yet and it's not
    /// added to the DAG.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn generate(
        &self,
        task: &IOITask,
        eval: &mut EvaluationData,
        description: String,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<(Option<FileUuid>, Option<Execution>), Error> {
        match self {
            OutputGenerator::NotAvailable => {
                let file = File::new("Empty file");
                let uuid = file.uuid;
                eval.dag.provide_content(file, vec![]);
                Ok((Some(uuid), None))
            }
            OutputGenerator::StaticFile(path) => {
                if !path.exists() {
                    bail!("Static output file not found: {:?}", path);
                }
                let file = File::new(format!(
                    "Static output file of testcase {subtask_id}, subtask {testcase_id} from {path:?}"
                ));
                let uuid = file.uuid;
                eval.dag.provide_file(file, path).with_context(|| {
                    format!(
                        "Failed to provide static output file from {}",
                        path.display()
                    )
                })?;
                Ok((Some(uuid), None))
            }
            OutputGenerator::Custom(source_file, args) => {
                let mut exec = source_file
                    .execute(eval, description, args.clone())
                    .context("Failed to execute output generator source file")?;
                exec.tag(Tag::Generation.into());
                exec.priority(GENERATION_PRIORITY - testcase_id as Priority);
                let output = bind_exec_io!(exec, task, input, validation_handle);
                Ok((Some(output.uuid), Some(exec)))
            }
        }
    }

    /// Add the generation of the output file to the DAG and the callbacks to the UI, returning the
    /// handle to the output file.
    pub(crate) fn generate_and_bind(
        &self,
        task: &IOITask,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<Option<FileUuid>, Error> {
        let (output, sol) = self.generate(
            task,
            eval,
            format!("Generation of output file of testcase {testcase_id}, subtask {subtask_id}"),
            subtask_id,
            testcase_id,
            input,
            validation_handle,
        )?;
        if let Some(mut sol) = sol {
            sol.capture_stderr(STDERR_CONTENT_LENGTH);
            bind_exec_callbacks!(eval, sol.uuid, |status| UIMessage::IOISolution {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            let sender = eval.sender.clone();
            eval.dag.on_execution_done(&sol.uuid, move |result| {
                if !result.status.is_success() {
                    let mut diagnostic =
                        Diagnostic::error(format!("Failed to generate output {testcase_id}"));
                    if let Some(stderr) = result.stderr {
                        diagnostic = diagnostic.with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            });
            eval.dag.add_execution(sol);
        }
        if let Some(output) = output {
            eval.dag.write_file_to(
                output,
                task.path
                    .join("output")
                    .join(format!("output{testcase_id}.txt")),
                false,
            );
        }
        Ok(output)
    }
}
