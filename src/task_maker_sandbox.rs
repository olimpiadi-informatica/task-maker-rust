use std::io::{stdin, stdout};

use anyhow::{Context, Error};
use tabox::result::SandboxExecutionResult;
use tabox::{Sandbox, SandboxImplementation};

use task_maker_exec::RawSandboxResult;

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
pub fn main() {
    match run_sandbox() {
        Ok(res) => {
            serde_json::to_writer(stdout(), &RawSandboxResult::Success(res))
                .expect("Failed to print result");
        }
        Err(e) => {
            serde_json::to_writer(stdout(), &RawSandboxResult::Error(e.to_string()))
                .expect("Failed to print result");
        }
    }
}
