use crate::ui::*;
use failure::Error;
use failure::_core::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use task_maker_dag::*;
use task_maker_lang::GraderMap;

/// The data for an evaluation, including the DAG and the UI channel.
pub struct EvaluationData {
    /// The DAG with the evaluation data.
    pub dag: ExecutionDAG,
    /// The sender of the UI.
    pub sender: Arc<Mutex<UIMessageSender>>,
}

impl EvaluationData {
    /// Crate a new EvaluationData returning the data and the receiving part of
    /// the UI chanel.
    pub fn new() -> (EvaluationData, UIChannelReceiver) {
        let (sender, receiver) = UIMessageSender::new();
        (
            EvaluationData {
                dag: ExecutionDAG::new(),
                sender: Arc::new(Mutex::new(sender)),
            },
            receiver,
        )
    }
}

/// What can send UIMessages.
pub trait UISender {
    fn send(&self, message: UIMessage) -> Result<(), Error>;
}

/// Implement .send(message) for Mutex<UIMessageSender> in order to do
/// `EvaluationData.sender.send(message)`. This will lock the mutex and send
/// the message to the UI.
impl UISender for Mutex<UIMessageSender> {
    fn send(&self, message: UIMessage) -> Result<(), Error> {
        self.lock().unwrap().send(message)
    }
}

/// Wrapper around [`task_maker_lang::SourceFile`](../task_maker_lang/struct.SourceFile.html) that
/// also sends to the UI the messages about the compilation, making the compilation completely
/// transparent to the `SourceFile`.
#[derive(Debug)]
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
    pub fn execute(
        &mut self,
        eval: &mut EvaluationData,
        description: &str,
        args: Vec<String>,
    ) -> Result<Execution, Error> {
        let (comp, exec) = self.base.execute(&mut eval.dag, description, args)?;
        // if there is the compilation, send to the UI the messages
        if let Some(comp_uuid) = comp {
            let (sender1, path1) = (eval.sender.clone(), self.path.clone());
            let (sender2, path2) = (eval.sender.clone(), self.path.clone());
            let (sender3, path3) = (eval.sender.clone(), self.path.clone());
            eval.dag.on_execution_start(&comp_uuid, move |worker| {
                sender1
                    .send(UIMessage::Compilation {
                        file: path1,
                        status: UIExecutionStatus::Started {
                            worker: worker.to_string(),
                        },
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
