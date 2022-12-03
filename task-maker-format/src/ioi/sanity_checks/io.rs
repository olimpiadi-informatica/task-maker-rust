use std::sync::{Arc, Mutex};

use crate::ioi::IOITask;
use crate::sanity_checks::SanityCheck;
use crate::ui::UIMessageSender;
use crate::{EvaluationData, UISender};
use anyhow::Error;
use task_maker_dag::FileUuid;
use task_maker_diagnostics::Diagnostic;

/// Check that the input and output files end with `\n`.
#[derive(Debug, Default)]
pub struct IOEndWithNewLine;

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
    fn new(eval: &mut EvaluationData, file: FileUuid, file_type: &str, path: &str) {
        let mut checker = Self {
            last_chunk_ends_with_new_line: true,
            sender: eval.sender.clone(),
            diagnostic_message: format!(
                "{} file at {} is not ending with a new-line",
                file_type, path
            ),
        };
        eval.dag
            .get_file_content_chunked(file, move |chunk| checker.add_chunk(chunk));
    }

    fn add_chunk(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if chunk.is_empty() {
            if !self.last_chunk_ends_with_new_line {
                self.sender.add_diagnostic(
                    Diagnostic::warning(&self.diagnostic_message)
                        .with_help("It's bad practice to have files not ending with new-line"),
                )?;
            }
        } else {
            self.last_chunk_ends_with_new_line = chunk.last().map(|&c| c == b'\n').unwrap_or(false);
        }
        Ok(())
    }
}

impl SanityCheck<IOITask> for IOEndWithNewLine {
    fn name(&self) -> &'static str {
        "IOEndWithNewLine"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        for subtask in task.subtasks.values() {
            for (&testcase_id, testcase) in &subtask.testcases {
                if let Some(input_file) = testcase.input_file {
                    CheckEndWithNewLine::new(
                        eval,
                        input_file,
                        "Input",
                        &format!("input/input{}.txt", testcase_id),
                    );
                }
                if let Some(output_file) = testcase.official_output_file {
                    CheckEndWithNewLine::new(
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
