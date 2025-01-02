use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Error};
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::{Execution, File, FileUuid, Priority};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::{SubtaskId, TestcaseId, GENERATION_PRIORITY, STDERR_CONTENT_LENGTH};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, UISender};
use crate::{EvaluationData, SourceFile, Tag};

/// The source of the input files. It can either be a statically provided input file or a custom
/// command that will generate an input file.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub enum InputGenerator {
    /// Use the static file as input. The file will be copied without transformations.
    StaticFile(PathBuf),
    /// Use a custom command to generate the input file. The file has to be printed to stdout.
    Custom(Arc<SourceFile>, Vec<String>),
}

impl InputGenerator {
    /// Build the execution for the generation of the input file. Return the handle to the input
    /// file and the `Execution` if any. The execution does not send UI messages yet and it's not
    /// added to the DAG.
    pub(crate) fn generate(
        &self,
        eval: &mut EvaluationData,
        task_path: &Path,
        description: String,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
    ) -> Result<(FileUuid, Option<Execution>), Error> {
        match self {
            InputGenerator::StaticFile(path) => {
                if !path.exists() {
                    bail!("COPY from not existing file: {:?}", path);
                }
                let file = File::new(format!(
                    "Static input file of testcase {}, subtask {} from {:?}",
                    subtask_id, testcase_id, path
                ));
                let uuid = file.uuid;
                eval.dag.provide_file(file, path).with_context(|| {
                    format!(
                        "Failed to provide static input file from {}",
                        path.display()
                    )
                })?;
                Ok((uuid, None))
            }
            InputGenerator::Custom(source_file, args) => {
                let mut exec = source_file
                    .execute(eval, description, args.clone())
                    .context("Failed to execute generator source file")?;

                exec.limits_mut().allow_multiprocess();
                exec.tag(Tag::Generation.into());
                exec.priority(GENERATION_PRIORITY - testcase_id as Priority);

                // Add limiti.yaml and constraints.yaml file to the sandbox of the generator
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
                Ok((stdout.uuid, Some(exec)))
            }
        }
    }

    /// Add the generation of the input file to the DAG and the callbacks to the UI, returning the
    /// handle to the input file.
    pub(crate) fn generate_and_bind(
        &self,
        eval: &mut EvaluationData,
        task_path: &Path,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
    ) -> Result<FileUuid, Error> {
        let (input, gen) = self.generate(
            eval,
            task_path,
            format!(
                "Generation of input file of testcase {}, subtask {}",
                testcase_id, subtask_id
            ),
            subtask_id,
            testcase_id,
        )?;
        eval.dag.write_file_to(
            input,
            eval.task_root
                .join("input")
                .join(format!("input{}.txt", testcase_id)),
            false,
        );
        // If there is an execution, bind its callbacks and store the input file.
        if let Some(mut gen) = gen {
            gen.capture_stderr(STDERR_CONTENT_LENGTH);
            bind_exec_callbacks!(eval, gen.uuid, |status| UIMessage::IOIGeneration {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            let sender = eval.sender.clone();
            let args = gen.args.join(" ");
            eval.dag.on_execution_done(&gen.uuid, move |result| {
                if !result.status.is_success() {
                    let mut diagnostic =
                        Diagnostic::error(format!("Failed to generate input {}", testcase_id))
                            .with_note(format!("Generator arguments are: {}", args));
                    if let Some(stderr) = result.stderr {
                        diagnostic = diagnostic.with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            });
            eval.dag.add_execution(gen);
        }
        Ok(input)
    }
}
