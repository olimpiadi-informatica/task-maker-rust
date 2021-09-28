use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::*;
use task_maker_lang::GraderMap;

use crate::bind_exec_callbacks;
use crate::ui::*;
use crate::EvaluationData;

/// Wrapper around [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) that
/// also sends to the UI the messages about the compilation, making the compilation completely
/// transparent to the `SourceFile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    #[serde(flatten)]
    base: task_maker_lang::SourceFile,
}

impl SourceFile {
    /// Make a new `SourceFile`. See
    /// [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) for the details.
    pub fn new<P: Into<PathBuf>, P2: Into<PathBuf>, P3: Into<PathBuf>>(
        path: P,
        base_path: P3,
        grader_map: Option<Arc<GraderMap>>,
        write_bin_to: Option<P2>,
    ) -> Option<SourceFile> {
        Some(SourceFile {
            base: task_maker_lang::SourceFile::new(path, base_path, grader_map, write_bin_to)?,
        })
    }

    /// Prepare an execution of the source file, eventually adding the compilation to the DAG.
    /// The compilation messages are sent to the UI.
    ///
    /// After the preparation the binary is executed with the specified arguments.
    ///
    /// See [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) for the
    /// details. Note that the return value is different because the compilation uuid is handled by
    /// this method.
    pub fn execute<S: AsRef<str>, S2: Into<String>, I: IntoIterator<Item = S2>>(
        &self,
        eval: &mut EvaluationData,
        description: S,
        args: I,
    ) -> Result<Execution, Error> {
        let (comp, exec) = self.base.execute(
            &mut eval.dag,
            description,
            args.into_iter().map(|s| s.into()).collect(),
        )?;
        self.bind_compilation_exe(eval, comp)?;
        Ok(exec)
    }

    /// Prepare an execution of the source file, eventually adding the compilation to the DAG.
    /// The compilation messages are sent to the UI.
    pub fn prepare(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        let comp = self.base.prepare(&mut eval.dag)?;
        self.bind_compilation_exe(eval, comp)?;
        Ok(())
    }

    /// Prepare the source file if needed and return the executable file.
    pub fn executable(&self, eval: &mut EvaluationData) -> Result<FileUuid, Error> {
        let (exe, comp) = self.base.executable(&mut eval.dag)?;
        self.bind_compilation_exe(eval, comp)?;
        Ok(exe)
    }

    /// Bind the callbacks for the compilation callbacks.
    fn bind_compilation_exe(
        &self,
        eval: &mut EvaluationData,
        comp: Option<ExecutionUuid>,
    ) -> Result<(), Error> {
        // if there is the compilation, send to the UI the messages
        if let Some(comp_uuid) = comp {
            let path = &self.path;
            bind_exec_callbacks!(
                eval,
                comp_uuid,
                |status, file| UIMessage::Compilation { file, status },
                path
            )?;
        }
        Ok(())
    }
}

impl Deref for SourceFile {
    type Target = task_maker_lang::SourceFile;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for SourceFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}
