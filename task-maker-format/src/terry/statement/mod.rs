use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use task_maker_dag::{Execution, ExecutionCommand, File};
use task_maker_diagnostics::Diagnostic;
use typescript_definitions::TypeScriptify;

use crate::{EvaluationData, Tag, UISender};
use anyhow::Error;

/// A statement is a markdown template together with subtask data
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct Statement {
    /// The path to the statement template
    pub path: PathBuf,
    /// The subtasks if they exist
    pub subtasks: Option<PathBuf>,
    /// The output path
    pub output: PathBuf,
}

impl Statement {
    pub fn generate_and_bind(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut exec = Execution::new(
            "Generation of the statement",
            ExecutionCommand::system("task-maker-tools"),
        );

        exec.limits_mut()
            .read_only(false)
            .allow_multiprocess()
            .mount_tmpfs(true);
        exec.tag(Tag::Booklet.into());

        let output = exec.output("output.md");

        let statement = File::new("Statement template");
        exec.input(&statement, "statement.in.md", false);
        eval.dag.provide_file(statement, &self.path)?;

        if let Some(subtasks_path) = &self.subtasks {
            let subtasks = File::new("Subtasks");
            exec.input(&subtasks, "subtasks.yaml", false);
            eval.dag.provide_file(subtasks, subtasks_path)?;

            exec.args(vec![
                "terry-statement",
                "-s",
                "statement.in.md",
                "-t",
                "subtasks.yaml",
                "-o",
                "output.md",
            ]);
        } else {
            exec.args(vec![
                "terry-statement",
                "-s",
                "statement.in.md",
                "-o",
                "output.md",
            ]);
        }

        let sender = eval.sender.clone();

        exec.capture_stderr(1024);
        eval.dag.on_execution_done(&exec.uuid, move |res| {
            if !res.status.is_success() {
                sender.add_diagnostic(Diagnostic::error(format!(
                    "Failed to generate statement.md. Generation stderr: {}",
                    String::from_utf8_lossy(&res.stderr.expect("Failed to capture stderr"))
                )))?;
            }
            Ok(())
        });

        eval.dag.add_execution(exec);
        eval.dag.write_file_to(output, &self.output, false);

        Ok(())
    }
}
