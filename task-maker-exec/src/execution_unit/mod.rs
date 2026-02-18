//! This mod contains execution-related code, both for sandboxed executions and un-sandboxed
//! executions which are done by task-maker directly

pub mod sandbox;
mod typst;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Error;
use serde::{Deserialize, Serialize};
use tabox::result::SandboxExecutionResult;
use task_maker_dag::*;
use task_maker_store::*;

use crate::execution_unit::sandbox::Sandbox;
use crate::execution_unit::typst::TypstCompiler;
use crate::sandbox_runner::SandboxRunner;
use crate::worker::OutputFile;

/// Result of the execution of the sandbox.
#[derive(Debug)]
pub enum SandboxResult {
    /// The sandbox exited successfully, the statistics about the sandboxed process are reported.
    Success {
        /// The exit status of the process.
        exit_status: u32,
        /// The signal that caused the process to exit.
        signal: Option<(u32, String)>,
        /// Resources used by the process.
        resources: ExecutionResourcesUsage,
        /// Whether the sandbox killed the process.
        was_killed: bool,
    },
    /// The sandbox failed to execute the process, an error message is reported. Note that this
    /// represents a sandbox error, not the process failure.
    Failed {
        /// The error reported by the sandbox.
        error: String,
    },
}

impl Default for SandboxResult {
    fn default() -> SandboxResult {
        SandboxResult::Success {
            exit_status: 0,
            resources: ExecutionResourcesUsage::default(),
            signal: None,
            was_killed: false,
        }
    }
}

/// Response of the internal implementation of the sandbox.
#[derive(Debug, Serialize, Deserialize)]
pub enum RawSandboxResult {
    /// The sandbox has been executed successfully.
    Success(SandboxExecutionResult),
    /// There was an error executing the sandbox.
    Error(String),
}

/// A singular execution, which can either be performed in a sandbox or be a Typst compilation
#[derive(Debug, Clone)]
pub enum ExecutionUnit {
    /// A sandboxed execution
    Sandbox(Sandbox),
    /// A Typst compilation
    TypstCompilation(Box<TypstCompiler>),
}

impl ExecutionUnit {
    /// Creates a new execution unit
    pub fn new(
        sandboxes_dir: &Path,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreHandle>,
        fifo_dir: Option<PathBuf>,
    ) -> Result<ExecutionUnit, Error> {
        if matches!(execution.command, ExecutionCommand::TypstCompilation { .. }) {
            TypstCompiler::new(Path::new("."), execution, dep_keys)
                .map(|typst_compiler| ExecutionUnit::TypstCompilation(Box::new(typst_compiler)))
        } else {
            Sandbox::new(sandboxes_dir, execution, dep_keys, fifo_dir).map(ExecutionUnit::Sandbox)
        }
    }

    /// Kills the process off the execution, if it is run in a sandbox
    pub fn kill(&self) {
        match self {
            ExecutionUnit::Sandbox(sandbox) => sandbox.kill(),
            ExecutionUnit::TypstCompilation(_) => {}
        }
    }

    /// Keeps the sandbox if one exists
    pub fn keep(&mut self) -> Result<(), Error> {
        match self {
            ExecutionUnit::Sandbox(sandbox) => sandbox.keep(),
            ExecutionUnit::TypstCompilation(_) => Ok(()),
        }
    }

    /// Runs the execution
    pub fn run(&mut self, runner: &dyn SandboxRunner) -> Result<SandboxResult, Error> {
        match self {
            ExecutionUnit::Sandbox(sandbox) => sandbox.run(runner),
            ExecutionUnit::TypstCompilation(typst_compiler) => typst_compiler.run(),
        }
    }

    /// Obtains the outputted standard output
    pub fn stdout_path(&self) -> OutputFile {
        match self {
            ExecutionUnit::Sandbox(sandbox) => OutputFile::OnDisk(sandbox.stdout_path()),
            ExecutionUnit::TypstCompilation(_) => OutputFile::InMemory(Vec::new()),
        }
    }

    /// Obtains the outputted standard error
    pub fn stderr_path(&self) -> OutputFile {
        match self {
            ExecutionUnit::Sandbox(sandbox) => OutputFile::OnDisk(sandbox.stderr_path()),
            ExecutionUnit::TypstCompilation(_) => OutputFile::InMemory(Vec::new()),
        }
    }

    /// Obtains the specified output file
    pub fn output_path(&self, output: &Path) -> OutputFile {
        match self {
            ExecutionUnit::Sandbox(sandbox) => OutputFile::OnDisk(sandbox.output_path(output)),
            ExecutionUnit::TypstCompilation(typst_compiler) => {
                OutputFile::InMemory(typst_compiler.output(output))
            }
        }
    }
}
