use failure::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use task_maker_dag::*;
use task_maker_store::*;
use tempdir::TempDir;

/// The list of all the system-wide readable directories inside the sandbox.
const READABLE_DIRS: &[&str] = &[
    "/lib",
    "/lib64",
    "/usr",
    "/bin",
    "/opt",
    // update-alternatives stuff, sometimes the executables are symlinked here
    "/etc/alternatives/",
    "/var/lib/dpkg/alternatives/",
];

/// Result of the execution of the sandbox.
#[derive(Debug)]
pub enum SandboxResult {
    /// The sandbox exited successfully, the statistics about the sandboxed process are reported.
    Success {
        /// The exit status of the process.
        exit_status: u32,
        /// The signal that caused the process to exit.
        signal: Option<u32>,
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

/// Internals of the sandbox.
#[derive(Debug)]
struct SandboxData {
    /// Handle to the temporary directory, will be deleted on drop. It's always Some(_) except
    /// inside `Drop`.
    boxdir: Option<TempDir>,
    /// Whether to keep the sandbox after exit.
    keep_sandbox: bool,
}

/// Wrapper around the sandbox. Cloning this struct will keep the reference of the same sandbox,
/// keeping the content alive.
///
/// This sandbox works only on Unix systems because it needs to set the executable bit on some
/// files.
#[derive(Debug, Clone)]
pub struct Sandbox {
    /// Internal data of the sandbox.
    data: Arc<Mutex<SandboxData>>,
    /// Execution to run.
    execution: Execution,
}

/// The outcome from `tmbox`. If the sandbox fails to run only `error` and `message` are set,
/// otherwise all the fields are present except for `message`.
#[derive(Debug, Deserialize)]
struct TMBoxResult {
    /// Whether the sandbox failed to execute the subprocess, will set `message`.
    error: bool,
    /// Error message from the sandbox.
    message: Option<String>,
    /// Total CPU time in user space, in seconds.
    cpu_time: Option<f64>,
    /// Total CPU time in kernel space, in seconds.
    sys_time: Option<f64>,
    /// Total time from the start to the end of the process, in seconds.
    wall_time: Option<f64>,
    /// Peak memory usage of the process in KiB.
    memory_usage: Option<u64>,
    /// Exit status code of the process.
    status_code: Option<u32>,
    /// Signal that made the process exit.
    signal: Option<u32>,
    /// Whether the sandbox killed the process.
    killed_by_sandbox: Option<bool>,
}

impl Sandbox {
    /// Make a new sandbox for the specified execution, copying all the required files. To start the
    /// sandbox call `run`.
    pub fn new(
        sandboxes_dir: &Path,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<Sandbox, Error> {
        std::fs::create_dir_all(sandboxes_dir)?;
        let boxdir = TempDir::new_in(sandboxes_dir, "box")?;
        Sandbox::setup(boxdir.path(), execution, dep_keys)?;
        Ok(Sandbox {
            data: Arc::new(Mutex::new(SandboxData {
                boxdir: Some(boxdir),
                keep_sandbox: false,
            })),
            execution: execution.clone(),
        })
    }

    /// Starts the sandbox and blocks the thread until the sandbox exits.
    pub fn run(&self) -> Result<SandboxResult, Error> {
        let boxdir = self.data.lock().unwrap().path().to_owned();
        trace!("Running sandbox at {:?}", boxdir);
        let tmbox_path = Path::new(env!("OUT_DIR")).join("bin").join("tmbox");
        let tmbox_path = if tmbox_path.exists() {
            tmbox_path
        } else {
            "tmbox".into()
        };
        let mut sandbox = Command::new(tmbox_path);
        sandbox.arg("--directory").arg(&boxdir.join("box"));
        sandbox.arg("--json");
        sandbox.arg("--env").arg("PATH");
        if self.execution.stdin.is_some() {
            sandbox.arg("--stdin").arg(boxdir.join("stdin"));
        } else {
            sandbox.arg("--stdin").arg("/dev/null");
        }
        if self.execution.stdout.is_some() {
            sandbox.arg("--stdout").arg(boxdir.join("stdout"));
        } else {
            sandbox.arg("--stdout").arg("/dev/null");
        }
        if self.execution.stderr.is_some() {
            sandbox.arg("--stderr").arg(boxdir.join("stderr"));
        } else {
            sandbox.arg("--stderr").arg("/dev/null");
        }
        for (key, value) in self.execution.env.iter() {
            sandbox.arg("--env").arg(format!("{}={}", key, value));
        }
        // set the cpu_limit (--time parameter) to the sum of cpu_time and
        // sys_time
        let mut cpu_limit = None;
        if let Some(cpu) = self.execution.limits.cpu_time {
            cpu_limit = Some(cpu);
        }
        if let Some(sys) = self.execution.limits.sys_time {
            if cpu_limit.is_none() {
                cpu_limit = Some(sys);
            } else {
                cpu_limit = Some(cpu_limit.unwrap() + sys);
            }
        }
        if let Some(cpu) = cpu_limit {
            let cpu = cpu + self.execution.config().extra_time;
            sandbox.arg("--time").arg(cpu.to_string());
        }
        if let Some(wall) = self.execution.limits.wall_time {
            let wall = wall + self.execution.config().extra_time;
            sandbox.arg("--wall").arg(wall.to_string());
        }
        match self.execution.limits.nproc {
            Some(1) => {}
            _ => {
                sandbox.arg("--multiprocess");
            }
        }
        // allow reading some basic directories
        for dir in READABLE_DIRS {
            if Path::new(dir).is_dir() {
                sandbox.arg("--readable-dir").arg(dir);
            }
        }
        sandbox.arg("--");
        match &self.execution.command {
            ExecutionCommand::System(cmd) => {
                if let Ok(cmd) = which::which(cmd) {
                    sandbox.arg(cmd)
                } else {
                    return Ok(SandboxResult::Failed {
                        error: format!("Executable {:?} not found", cmd),
                    });
                }
            }
            ExecutionCommand::Local(cmd) => sandbox.arg(cmd),
        };
        for arg in self.execution.args.iter() {
            sandbox.arg(arg);
        }
        trace!("Sandbox command: {:?}", sandbox);
        let res = sandbox.output()?;
        trace!("Sandbox output: {:?}", res);
        let outcome = serde_json::from_str::<TMBoxResult>(std::str::from_utf8(&res.stdout)?)?;
        if outcome.error {
            Ok(SandboxResult::Failed {
                error: outcome.message.unwrap(),
            })
        } else {
            let signal = if outcome.signal.unwrap() == 0 {
                None
            } else {
                Some(outcome.signal.unwrap())
            };
            Ok(SandboxResult::Success {
                exit_status: outcome.status_code.unwrap(),
                signal,
                resources: ExecutionResourcesUsage {
                    cpu_time: outcome.cpu_time.unwrap(),
                    sys_time: outcome.sys_time.unwrap(),
                    wall_time: outcome.wall_time.unwrap(),
                    memory: outcome.memory_usage.unwrap(),
                },
                was_killed: outcome.killed_by_sandbox.unwrap(),
            })
        }
    }

    /// Tell the sandbox process to kill the underlying process, this will make `run` terminate more
    /// quickly.
    pub fn kill(&self) {
        info!(
            "Sandbox at {:?} got killed",
            self.data.lock().unwrap().path()
        );
        unimplemented!();
    }

    /// Make the sandbox persistent, the sandbox directory won't be deleted after the execution.
    pub fn keep(&mut self) {
        let mut data = self.data.lock().unwrap();
        let path = data.boxdir.as_ref().unwrap().path().to_owned();
        debug!("Keeping sandbox at {:?}", path);
        data.keep_sandbox = true;
        let serialized =
            serde_json::to_string_pretty(&self.execution).expect("Cannot serialize execution");
        std::fs::write(path.join("info.json"), serialized)
            .expect("Cannot write execution info inside sandbox");
    }

    /// Path of the file where the standard output is written to.
    pub fn stdout_path(&self) -> PathBuf {
        self.data.lock().unwrap().path().join("stdout")
    }

    /// Path of the file where the standard error is written to.
    pub fn stderr_path(&self) -> PathBuf {
        self.data.lock().unwrap().path().join("stderr")
    }

    /// Path of the file where that output file is written to.
    pub fn output_path(&self, output: &Path) -> PathBuf {
        self.data.lock().unwrap().path().join("box").join(output)
    }

    /// Setup the sandbox directory with all the files required for the execution.
    fn setup<P: AsRef<Path>>(
        box_dir: P,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        trace!(
            "Setting up sandbox at {:?} for '{}'",
            box_dir.as_ref(),
            execution.description
        );
        std::fs::create_dir_all(box_dir.as_ref().join("box"))?;
        if let Some(stdin) = execution.stdin {
            Sandbox::write_sandbox_file(
                &box_dir.as_ref().join("stdin"),
                dep_keys.get(&stdin).expect("stdin not provided").path(),
                false,
            )?;
        }
        if execution.stdout.is_some() {
            Sandbox::touch_file(&box_dir.as_ref().join("stdout"), 0o600)?;
        }
        if execution.stderr.is_some() {
            Sandbox::touch_file(&box_dir.as_ref().join("stderr"), 0o600)?;
        }
        for (path, input) in execution.inputs.iter() {
            Sandbox::write_sandbox_file(
                &box_dir.as_ref().join("box").join(&path),
                dep_keys.get(&input.file).expect("file not provided").path(),
                input.executable,
            )?;
        }
        for path in execution.outputs.keys() {
            Sandbox::touch_file(&box_dir.as_ref().join("box").join(&path), 0o600)?;
        }
        // remove the write bit on the box folder
        if execution.limits.read_only {
            Sandbox::set_permissions(&box_dir.as_ref().join("box"), 0o500)?;
        }
        trace!("Sandbox at {:?} ready!", box_dir.as_ref());
        Ok(())
    }

    /// Put a file inside the sandbox, creating the directories if needed and making it executable
    /// if needed.
    ///
    /// The file will have the most restrictive permissions possible:
    /// - `r--------` (0o400) if not executable.
    /// - `r-x------` (0o500) if executable.
    fn write_sandbox_file(dest: &Path, source: &Path, executable: bool) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(source, dest)?;
        if executable {
            Sandbox::set_permissions(dest, 0o500)?;
        } else {
            Sandbox::set_permissions(dest, 0o400)?;
        }
        Ok(())
    }

    /// Create an empty file inside the sandbox and chmod-it.
    fn touch_file(dest: &Path, mode: u32) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::File::create(dest)?;
        let mut permisions = std::fs::metadata(&dest)?.permissions();
        permisions.set_mode(mode);
        std::fs::set_permissions(dest, permisions)?;
        Ok(())
    }

    fn set_permissions(dest: &Path, perm: u32) -> Result<(), Error> {
        let mut permissions = std::fs::metadata(&dest)?.permissions();
        permissions.set_mode(perm);
        std::fs::set_permissions(dest, permissions)?;
        Ok(())
    }
}

impl SandboxData {
    fn path(&self) -> &Path {
        // this unwrap is safe since only `Drop` will remove the boxdir
        self.boxdir.as_ref().unwrap().path()
    }
}

impl Drop for SandboxData {
    fn drop(&mut self) {
        if self.keep_sandbox {
            // this will unwrap the directory, dropping the `TempDir` without deleting the directory
            self.boxdir.take().map(TempDir::into_path);
        } else if Sandbox::set_permissions(&self.boxdir.as_ref().unwrap().path().join("box"), 0o700)
            .is_err()
        {
            warn!("Cannot 'chmod 700' the sandbox directory");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Sandbox;
    use std::collections::HashMap;
    use std::path::Path;
    use task_maker_dag::{Execution, ExecutionCommand};

    #[test]
    fn test_remove_sandbox_on_drop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let mut exec = Execution::new("test", ExecutionCommand::System("true".into()));
        exec.output("fooo");
        exec.limits_mut().read_only = true;
        let sandbox = Sandbox::new(tmpdir.path(), &exec, &HashMap::new()).unwrap();
        let outfile = sandbox.output_path(Path::new("fooo"));
        sandbox.run().unwrap();
        drop(sandbox);
        assert!(!outfile.exists());
        assert!(!outfile.parent().unwrap().exists()); // the box/ dir
        assert!(!outfile.parent().unwrap().parent().unwrap().exists()); // the sandbox dir
    }
}
