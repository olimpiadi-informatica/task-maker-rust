use std::fmt::Display;
use std::sync::{Arc, Mutex};

use crate::ioi::IOITask;
use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::ui::UIMessageSender;
use crate::{EvaluationData, UISender};
use anyhow::Error;
use task_maker_dag::FileUuid;
use task_maker_diagnostics::Diagnostic;

/// Check that the input and output files end with `\n`.
#[derive(Debug, Default)]
pub struct IOEndWithNewLine;
make_sanity_check!(IOEndWithNewLine);

/// Check that a file ends with `\n` and emit a warning if it doesn't. An empty file is considered
/// valid.
#[derive(Debug)]
pub struct CheckEndWithNewLine {
    /// Whether the last chunk ends with a new line.
    last_chunk_ends_with_new_line: bool,
    /// The sender used to emit the warning.
    sender: Arc<Mutex<UIMessageSender>>,
    /// The warning message to emit.
    diagnostic_message: String,
}

impl CheckEndWithNewLine {
    pub fn new(eval: &mut EvaluationData, file_type: &str, path: impl Display) -> Self {
        Self {
            last_chunk_ends_with_new_line: true,
            sender: eval.sender.clone(),
            diagnostic_message: format!(
                "{} file at {} does not end with a new-line",
                file_type, path
            ),
        }
    }

    pub fn bind(eval: &mut EvaluationData, file: FileUuid, file_type: &str, path: impl Display) {
        let mut checker = Self::new(eval, file_type, path);
        eval.dag
            .get_file_content_chunked(file, move |chunk| checker.add_chunk(chunk));
    }

    pub fn add_chunk(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if chunk.is_empty() {
            if !self.last_chunk_ends_with_new_line {
                self.sender
                    .add_diagnostic(Diagnostic::warning(&self.diagnostic_message).with_note(
                        "It's bad practice to have files that do not end with new-line",
                    ))?;
            }
        } else {
            self.last_chunk_ends_with_new_line = chunk.last().map(|&c| c == b'\n').unwrap_or(false);
        }
        Ok(())
    }
}

impl SanityCheck for IOEndWithNewLine {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "IOEndWithNewLine"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Io
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        for subtask in task.subtasks.values() {
            for (&testcase_id, testcase) in &subtask.testcases {
                if let Some(input_file) = testcase.input_file {
                    CheckEndWithNewLine::bind(
                        eval,
                        input_file,
                        "Input",
                        &format!("input/input{}.txt", testcase_id),
                    );
                }
                if let Some(output_file) = testcase.official_output_file {
                    CheckEndWithNewLine::bind(
                        eval,
                        output_file,
                        "Official output",
                        &format!("output/output{}.txt", testcase_id),
                    );
                }
            }
        }
        Ok(())
    }
}
