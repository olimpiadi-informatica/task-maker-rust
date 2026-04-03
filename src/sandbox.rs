use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Error};
use tabox::configuration::SandboxConfiguration;
use tabox::result::SandboxExecutionResult;
use tabox::{Sandbox, SandboxImplementation};
use task_maker_exec::find_tools::find_tools_path;
use task_maker_exec::{RawSandboxResult, SandboxRunner};
use tempfile::NamedTempFile;

fn run_sandbox(config: &str) -> Result<SandboxExecutionResult, Error> {
    let config = serde_json::from_str(config).context("Cannot parse configuration")?;
    let sandbox = SandboxImplementation::run(config).context("Failed to create sandbox")?;
    let res = sandbox.wait().context("Failed to wait sandbox")?;
    Ok(res)
}

/// Run the sandbox for an execution.
pub fn main_sandbox() {
    let mut args = env::args().skip(2);
    let configuration = args.next().unwrap();
    let output_file = args.next().unwrap();
    let result = match run_sandbox(&configuration) {
        Ok(res) => RawSandboxResult::Success(res),
        Err(e) => {
            let err = format!("Error: {e:?}");
            RawSandboxResult::Error(err)
        }
    };
    let f = File::options()
        .write(true)
        .open(&output_file)
        .expect("Failed to create output file");
    serde_json::to_writer(BufWriter::new(f), &result).expect("Failed to print result");
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
    let config = serde_json::to_string(&config).context("Failed to serialize config")?;
    // TODO(veluca): it would be nice to write the result in the sandbox.
    let outfile = NamedTempFile::new().context("Failed creating output tempfile")?;
    let mut cmd = Command::new(tools_path)
        .arg("internal-sandbox")
        .arg(config)
        .arg(outfile.path().as_os_str())
        .spawn()
        .context("Cannot spawn the sandbox")?;
    pid.store(cmd.id(), Ordering::SeqCst);
    let status = cmd.wait().context("Failed to wait for the process")?;
    if !status.success() {
        bail!("Sandbox process failed: {}", status.to_string());
    }
    serde_json::from_reader(outfile).context("Invalid output from sandbox")
}
