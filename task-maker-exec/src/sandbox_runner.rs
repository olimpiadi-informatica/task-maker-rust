use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use tabox::configuration::SandboxConfiguration;

use crate::RawSandboxResult;
use tabox::result::{ExitStatus, ResourceUsage, SandboxExecutionResult};

/// Something able to spawn a sandbox, wait for it to exit and return the results.
pub trait SandboxRunner: Send + Sync {
    /// Spawn a sandbox with the provided configuration, set the PID as soon as possible and wait
    /// for it to exit. Parse the outcome of the sandbox and return it.
    fn run(&self, config: SandboxConfiguration, pid: Arc<AtomicU32>) -> RawSandboxResult;
}

/// A fake sandbox that don't actually spawn anything and always return an error.
#[derive(Default, Debug)]
pub struct ErrorSandboxRunner;

impl SandboxRunner for ErrorSandboxRunner {
    fn run(&self, _config: SandboxConfiguration, _pid: Arc<AtomicU32>) -> RawSandboxResult {
        RawSandboxResult::Error("Nope".to_owned())
    }
}

/// A fake sandbox that don't actually spawn anything and always return successfully with exit code
/// 0.
#[derive(Default, Debug)]
pub struct SuccessSandboxRunner;

impl SandboxRunner for SuccessSandboxRunner {
    fn run(&self, _config: SandboxConfiguration, _pid: Arc<AtomicU32>) -> RawSandboxResult {
        RawSandboxResult::Success(SandboxExecutionResult {
            status: ExitStatus::ExitCode(0),
            resource_usage: ResourceUsage {
                memory_usage: 0,
                user_cpu_time: 0.0,
                system_cpu_time: 0.0,
                wall_time_usage: 0.0,
            },
        })
    }
}

/// A fake sandbox that don't actually spawn anything and return with success, if the command was
/// `true` the exit code is zero, otherwise it's 1.
#[derive(Default, Debug)]
pub struct FakeSandboxRunner;

impl SandboxRunner for FakeSandboxRunner {
    fn run(&self, config: SandboxConfiguration, _pid: Arc<AtomicU32>) -> RawSandboxResult {
        let resource_usage = ResourceUsage {
            memory_usage: 0,
            user_cpu_time: 0.0,
            system_cpu_time: 0.0,
            wall_time_usage: 0.0,
        };
        if config.executable.ends_with("true") {
            RawSandboxResult::Success(SandboxExecutionResult {
                status: ExitStatus::ExitCode(0),
                resource_usage,
            })
        } else {
            RawSandboxResult::Success(SandboxExecutionResult {
                status: ExitStatus::ExitCode(1),
                resource_usage,
            })
        }
    }
}

impl<S: SandboxRunner> SandboxRunner for Arc<S> {
    fn run(&self, conf: SandboxConfiguration, pid: Arc<AtomicU32>) -> RawSandboxResult {
        self.as_ref().run(conf, pid)
    }
}
