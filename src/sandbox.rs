use std::io::{stdin, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Error};
use tabox::configuration::SandboxConfiguration;
use tabox::result::SandboxExecutionResult;
use tabox::{Sandbox, SandboxImplementation};

use task_maker_exec::find_tools::find_tools_path;
use task_maker_exec::{RawSandboxResult, SandboxRunner};

/// Actually parse the input and return the result.
fn run_sandbox() -> Result<SandboxExecutionResult, Error> {
    let config =
        serde_json::from_reader(stdin()).context("Cannot read configuration from stdin")?;
    let sandbox = SandboxImplementation::run(config).context("Failed to create sandbox")?;
    let res = sandbox.wait().context("Failed to wait sandbox")?;
    Ok(res)
}

/// Run the sandbox for an execution.
///
/// It takes a `SandboxConfiguration`, JSON serialized via standard input and prints to standard
/// output a `RawSandboxResult`, JSON serialized.
pub fn main_sandbox() {
    match run_sandbox() {
        Ok(res) => {
            serde_json::to_writer(stdout(), &RawSandboxResult::Success(res))
                .expect("Failed to print result");
        }
        Err(e) => {
            let err = format!("Error: {e:?}");
            serde_json::to_writer(stdout(), &RawSandboxResult::Error(err))
                .expect("Failed to print result");
        }
    }
}

/// Run the sandbox integrated in the task-maker-tools binary.
#[derive(Clone, Debug)]
pub struct ToolsSandboxRunner {
    /// Path to the tools executable.
    tools_path: PathBuf,
}

impl Default for ToolsSandboxRunner {
    fn default() -> Self {
        ToolsSandboxRunner {
            tools_path: find_tools_path(),
        }
    }
}

impl SandboxRunner for ToolsSandboxRunner {
    fn run(&self, config: SandboxConfiguration, pid: Arc<AtomicU32>) -> RawSandboxResult {
        match tools_sandbox_internal(&self.tools_path, config, pid) {
            Ok(res) => res,
            Err(e) => RawSandboxResult::Error(e.to_string()),
        }
    }
}

/// Actually run the sandbox, but with a return type that supports the `?` operator.
fn tools_sandbox_internal(
    tools_path: &Path,
    config: SandboxConfiguration,
    pid: Arc<AtomicU32>,
) -> Result<RawSandboxResult, Error> {
    let mut cmd = Command::new(tools_path)
        .arg("internal-sandbox")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Cannot spawn the sandbox")?;
    pid.store(cmd.id(), Ordering::SeqCst);
    {
        let stdin = cmd.stdin.as_mut().context("Failed to open stdin")?;
        serde_json::to_writer(stdin, &config.build()).context("Failed to write config to stdin")?;
    }
    let output = cmd
        .wait_with_output()
        .context("Failed to wait for the process")?;
    if !output.status.success() {
        bail!(
            "Sandbox process failed: {}\n{}",
            output.status.to_string(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("Invalid output from sandbox")
}
