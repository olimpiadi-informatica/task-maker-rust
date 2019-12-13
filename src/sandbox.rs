use std::io::{stdin, stdout};

use failure::{bail, format_err, Error};
use tabox::result::SandboxExecutionResult;
use tabox::{Sandbox, SandboxImplementation};

use std::process::{Command, Stdio};
use tabox::configuration::SandboxConfiguration;
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

/// Run the sandbox integrated in the task-maker binary, by executing itself with different command
/// line arguments.
pub fn self_exec_sandbox(config: SandboxConfiguration) -> RawSandboxResult {
    match self_exec_sandbox_internal(config) {
        Ok(res) => res,
        Err(e) => RawSandboxResult::Error(e.to_string()),
    }
}

/// Actually run the sandbox, but with a return type that supports the `?` operator.
fn self_exec_sandbox_internal(config: SandboxConfiguration) -> Result<RawSandboxResult, Error> {
    let mut cmd = Command::new(std::env::current_exe()?)
        .arg("--sandbox")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let stdin = cmd.stdin.as_mut().expect("Failed to open stdin");
        serde_json::to_writer(stdin, &config.build())?;
    }
    let output = cmd.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "Sandbox failed with code: {:?}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}
