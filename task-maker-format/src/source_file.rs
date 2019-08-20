use crate::ui::*;
use crate::EvaluationData;
use crate::UISender;
use failure::Error;
use failure::_core::ops::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use task_maker_dag::*;
use task_maker_lang::GraderMap;

/// Wrapper around [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) that
/// also sends to the UI the messages about the compilation, making the compilation completely
/// transparent to the `SourceFile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    base: task_maker_lang::SourceFile,
}

impl SourceFile {
    /// Make a new `SourceFile`. See
    /// [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) for the details.
    pub fn new<P: Into<PathBuf>>(
        path: P,
        grader_map: Option<Arc<GraderMap>>,
    ) -> Option<SourceFile> {
        Some(SourceFile {
            base: task_maker_lang::SourceFile::new(path, grader_map)?,
        })
    }

    /// Prepare an execution of the source file, eventually adding the compilation to the DAG.
    /// The compilation messages are sent to the UI.
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
        // if there is the compilation, send to the UI the messages
        if let Some(comp_uuid) = comp {
            let (sender1, path1) = (eval.sender.clone(), self.path.clone());
            let (sender2, path2) = (eval.sender.clone(), self.path.clone());
            let (sender3, path3) = (eval.sender.clone(), self.path.clone());
            eval.dag.on_execution_start(&comp_uuid, move |worker| {
                sender1
                    .send(UIMessage::Compilation {
                        file: path1,
                        status: UIExecutionStatus::Started { worker },
                    })
                    .unwrap();
            });
            eval.dag.on_execution_done(&comp_uuid, move |result| {
                sender2
                    .send(UIMessage::Compilation {
                        file: path2,
                        status: UIExecutionStatus::Done { result },
                    })
                    .unwrap();
            });
            eval.dag.on_execution_skip(&comp_uuid, move || {
                sender3
                    .send(UIMessage::Compilation {
                        file: path3,
                        status: UIExecutionStatus::Skipped,
                    })
                    .unwrap();
            });
            eval.sender
                .send(UIMessage::Compilation {
                    file: self.path.clone(),
                    status: UIExecutionStatus::Pending,
                })
                .unwrap();
        }
        Ok(exec)
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
