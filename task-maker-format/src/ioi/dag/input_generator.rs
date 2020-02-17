use std::path::PathBuf;
use std::sync::Arc;

use failure::{bail, Error};
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, File, FileUuid, Priority};

use crate::bind_exec_callbacks;
use crate::ioi::{SubtaskId, TestcaseId, GENERATION_PRIORITY, STDERR_CONTENT_LENGTH};
use crate::ui::UIMessage;
use crate::{EvaluationData, SourceFile, Tag, UISender};

/// The source of the input files. It can either be a statically provided input file or a custom
/// command that will generate an input file.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
                eval.dag.provide_file(file, &path)?;
                Ok((uuid, None))
            }
            InputGenerator::Custom(source_file, args) => {
                let mut exec = source_file.execute(eval, description, args.clone())?;
                exec.tag(Tag::Generation.into());
                exec.priority(GENERATION_PRIORITY - testcase_id as Priority);
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
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
    ) -> Result<FileUuid, Error> {
        let (input, gen) = self.generate(
            eval,
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
        // if there is an execution, bind its callbacks and store the input file
        if let Some(mut gen) = gen {
            bind_exec_callbacks!(eval, gen.uuid, |status| UIMessage::IOIGeneration {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            let sender = eval.sender.clone();
            eval.dag
                .get_file_content(gen.stderr(), STDERR_CONTENT_LENGTH, move |content| {
                    let content = String::from_utf8_lossy(&content);
                    sender.send(UIMessage::IOIGenerationStderr {
                        testcase: testcase_id,
                        subtask: subtask_id,
                        content: content.into(),
                    })
                });
            eval.dag.add_execution(gen);
        }
        Ok(input)
    }
}
