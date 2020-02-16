use std::path::PathBuf;
use std::sync::Arc;

use failure::{bail, Error};
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, File, FileUuid, Priority};

use crate::ioi::{SubtaskId, Tag, Task, TestcaseId, GENERATION_PRIORITY};
use crate::ui::UIMessage;
use crate::{bind_exec_callbacks, bind_exec_io};
use crate::{EvaluationData, SourceFile};

/// The source of the output files. It can either be a statically provided output file or a custom
/// command that will generate an output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputGenerator {
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
        task: &Task,
        eval: &mut EvaluationData,
        description: String,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<(FileUuid, Option<Execution>), Error> {
        match self {
            OutputGenerator::StaticFile(path) => {
                if !path.exists() {
                    bail!("Static output file not found: {:?}", path);
                }
                let file = File::new(format!(
                    "Static output file of testcase {}, subtask {} from {:?}",
                    subtask_id, testcase_id, path
                ));
                let uuid = file.uuid;
                eval.dag.provide_file(file, &path)?;
                Ok((uuid, None))
            }
            OutputGenerator::Custom(source_file, args) => {
                let mut exec = source_file.execute(eval, description, args.clone())?;
                exec.tag(Tag::Generation.into());
                exec.priority(GENERATION_PRIORITY - testcase_id as Priority);
                let output = bind_exec_io!(exec, task, input, validation_handle);
                Ok((output.uuid, Some(exec)))
            }
        }
    }

    /// Add the generation of the output file to the DAG and the callbacks to the UI, returning the
    /// handle to the output file.
    pub(crate) fn generate_and_bind(
        &self,
        task: &Task,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<FileUuid, Error> {
        let (output, sol) = self.generate(
            task,
            eval,
            format!(
                "Generation of output file of testcase {}, subtask {}",
                testcase_id, subtask_id
            ),
            subtask_id,
            testcase_id,
            input,
            validation_handle,
        )?;
        if let Some(sol) = sol {
            bind_exec_callbacks!(eval, sol.uuid, |status| UIMessage::IOISolution {
                subtask: subtask_id,
                testcase: testcase_id,
                status
            })?;
            eval.dag.add_execution(sol);
        }
        eval.dag.write_file_to(
            output,
            task.path
                .join("output")
                .join(format!("output{}.txt", testcase_id)),
            false,
        );
        Ok(output)
    }
}
