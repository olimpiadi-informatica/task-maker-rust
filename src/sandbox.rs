use std::io::{stdin, stdout};

use failure::{format_err, Error};
use tabox::result::SandboxExecutionResult;
use tabox::{Sandbox, SandboxImplementation};

use task_maker_exec::RawSandboxResult;

/// Actually parse the input and return the result.
fn run_sandbox() -> Result<SandboxExecutionResult, Error> {
    let config = serde_json::from_reader(stdin())?;
    let sandbox = SandboxImplementation::run(config)
        .map_err(|e| format_err!("Failed to create sandbox: {:?}", e))?;
    let res = sandbox
        .wait()
        .map_err(|e| format_err!("Failed to wait sandbox: {:?}", e))?;
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
            serde_json::to_writer(stdout(), &RawSandboxResult::Error(e.to_string()))
                .expect("Failed to print result");
        }
    }
}
