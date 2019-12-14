use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use failure::{bail, Error};
use serde::{Deserialize, Serialize};
use tabox::configuration::SandboxConfiguration;
use tabox::result::SandboxExecutionResult;
use tabox::syscall_filter::SyscallFilter;
use tempdir::TempDir;

use task_maker_dag::*;
use task_maker_store::*;

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
        // TODO make these an enum since there are three disjoint cases: exit with status, killed by
        //  signal, killed by sandbox
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

/// Response of the internal implementation of the sandbox.
#[derive(Debug, Serialize, Deserialize)]
pub enum RawSandboxResult {
    /// The sandbox has been executed successfully.
    Success(SandboxExecutionResult),
    /// There was an error executing the sandbox.
    Error(String),
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
    pub fn run<F>(&self, runner: F) -> Result<SandboxResult, Error>
    where
        F: FnOnce(SandboxConfiguration) -> RawSandboxResult,
    {
        let boxdir = self.data.lock().unwrap().path().to_owned();
        trace!("Running sandbox at {:?}", boxdir);

        let mut config = SandboxConfiguration::default();
        if let Err(e) = self.build_command(&boxdir, &mut config) {
            return Ok(SandboxResult::Failed { error: e });
        }
        trace!("Sandbox configuration: {:#?}", config);

        let raw_result = runner(config.build());
        let res = match raw_result {
            RawSandboxResult::Success(res) => res,
            RawSandboxResult::Error(e) => bail!("Sandbox failed: {}", e),
        };
        trace!("Sandbox output: {:?}", res);

        let resources = ExecutionResourcesUsage {
            cpu_time: res.resource_usage.user_cpu_time,
            sys_time: res.resource_usage.system_cpu_time,
            wall_time: res.resource_usage.wall_time_usage,
            memory: res.resource_usage.memory_usage as u64 / 1024,
        };

        use tabox::result::ExitStatus::*;
        match res.status {
            ExitCode(code) => Ok(SandboxResult::Success {
                exit_status: code as u32,
                signal: None,
                resources,
                was_killed: false,
            }),
            Signal(s) => Ok(SandboxResult::Success {
                exit_status: 0,
                signal: Some(s as u32),
                resources,
                was_killed: false,
            }),
            Killed => Ok(SandboxResult::Success {
                exit_status: 1,
                signal: Some(9),
                resources,
                was_killed: true,
            }),
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
        let path = data
            .boxdir
            .as_ref()
            .expect("Box dir has gone?!?")
            .path()
            .to_owned();
        debug!("Keeping sandbox at {:?}", path);
        data.keep_sandbox = true;
        let serialized =
            serde_json::to_string_pretty(&self.execution).expect("Cannot serialize execution");
        std::fs::write(path.join("info.json"), serialized)
            .expect("Cannot write execution info inside sandbox");
        let mut config = SandboxConfiguration::default();
        if let Ok(()) = self.build_command(&path, &mut config) {
            std::fs::write(path.join("tabox.txt"), format!("{:#?}\n", config))
                .expect("Cannot write command info inside sandbox");
        }
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

    /// Build the command line arguments of `tmbox`.
    fn build_command(
        &self,
        boxdir: &Path,
        config: &mut SandboxConfiguration,
    ) -> Result<(), String> {
        config.working_directory(boxdir.join("box"));
        // the box directory must be writable otherwise the output files cannot be written
        config.mount(boxdir.join("box"), boxdir.join("box"), true);
        config.env("PATH", std::env::var("PATH").unwrap_or_default());
        if self.execution.stdin.is_some() {
            config.stdin(boxdir.join("stdin"));
        } else {
            config.stdin("/dev/null");
        }
        if self.execution.stdout.is_some() {
            config.stdout(boxdir.join("stdout"));
        } else {
            config.stdout("/dev/null");
        }
        if self.execution.stderr.is_some() {
            config.stderr(boxdir.join("stderr"));
        } else {
            config.stderr("/dev/null");
        }
        for (key, value) in self.execution.env.iter() {
            config.env(key, value);
        }

        let cpu_limit = match (
            self.execution.limits.cpu_time,
            self.execution.limits.sys_time,
        ) {
            (Some(cpu), Some(sys)) => Some(cpu + sys),
            (Some(cpu), None) => Some(cpu),
            (None, Some(sys)) => Some(sys),
            (None, None) => None,
        };
        if let Some(cpu) = cpu_limit {
            let cpu = cpu + self.execution.config().extra_time;
            config.time_limit(cpu.ceil() as u64);
        }
        if let Some(wall) = self.execution.limits.wall_time {
            let wall = wall + self.execution.config().extra_time;
            config.wall_time_limit(wall.ceil() as u64);
        }
        if let Some(mem) = self.execution.limits.memory {
            config.memory_limit(mem * 1024);
        }
        let multiproc = Some(1) != self.execution.limits.nproc;
        config.syscall_filter(SyscallFilter::build(
            multiproc,
            !self.execution.limits.read_only,
        ));
        for dir in READABLE_DIRS {
            if Path::new(dir).is_dir() {
                config.mount(dir, dir, false);
            }
        }
        for dir in &self.execution.limits.extra_readable_dirs {
            if dir.is_dir() {
                config.mount(dir, dir, false);
            }
        }
        if self.execution.limits.mount_tmpfs {
            config.mount_tmpfs(true);
        }
        match &self.execution.command {
            ExecutionCommand::System(cmd) => {
                if let Ok(cmd) = which::which(cmd) {
                    config.executable(cmd);
                } else {
                    return Err(format!("Executable {:?} not found", cmd));
                }
            }
            ExecutionCommand::Local(cmd) => {
                config.executable(boxdir.join("box").join(cmd));
            }
        };
        for arg in self.execution.args.iter() {
            config.arg(arg);
        }
        Ok(())
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
        std::fs::create_dir_all(dest.parent().expect("Invalid destination path"))?;
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
        std::fs::create_dir_all(dest.parent().expect("Invalid file path"))?;
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
    use std::collections::HashMap;
    use std::path::Path;

    use task_maker_dag::{Execution, ExecutionCommand};

    use crate::sandbox::Sandbox;
    use crate::RawSandboxResult;
    use tabox::configuration::{DirectoryMount, SandboxConfiguration};
    use tabox::syscall_filter::SyscallFilterAction;

    fn fake_sandbox(_: SandboxConfiguration) -> RawSandboxResult {
        RawSandboxResult::Error("Nope".to_owned())
    }

    #[test]
    fn test_remove_sandbox_on_drop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let mut exec = Execution::new("test", ExecutionCommand::system("true"));
        exec.output("fooo");
        exec.limits_mut().read_only(true);
        let sandbox = Sandbox::new(tmpdir.path(), &exec, &HashMap::new()).unwrap();
        let outfile = sandbox.output_path(Path::new("fooo"));
        if let Err(e) = sandbox.run(&fake_sandbox) {
            assert!(e.to_string().contains("Nope"));
        } else {
            panic!("Sandbox not called");
        }
        drop(sandbox);
        assert!(!outfile.exists());
        assert!(!outfile.parent().unwrap().exists()); // the box/ dir
        assert!(!outfile.parent().unwrap().parent().unwrap().exists()); // the sandbox dir
    }

    #[test]
    fn test_command_args() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let mut exec = Execution::new("test", ExecutionCommand::local("foo"));
        exec.args(vec!["bar", "baz"]);
        exec.limits_mut()
            .sys_time(1.0)
            .cpu_time(2.6)
            .wall_time(10.0)
            .mount_tmpfs(true)
            .add_extra_readable_dir("/home")
            .nproc(2)
            .memory(1234);
        exec.env("foo", "bar");
        let sandbox = Sandbox::new(tmpdir.path(), &exec, &HashMap::new()).unwrap();
        let mut config = SandboxConfiguration::default();
        sandbox.build_command(tmpdir.path(), &mut config).unwrap();
        let extra_time = exec.config().extra_time;
        let total_time = (1.0 + 2.6 + extra_time).ceil() as u64;
        let wall_time = (10.0 + extra_time).ceil() as u64;
        let boxdir = tmpdir.path().join("box");
        assert_eq!(config.working_directory, boxdir);
        assert_eq!(config.time_limit, Some(total_time));
        assert_eq!(config.wall_time_limit, Some(wall_time));
        assert_eq!(config.memory_limit, Some(1234 * 1024));
        assert!(config.mount_paths.contains(&DirectoryMount {
            target: "/home".into(),
            source: "/home".into(),
            writable: false
        }));
        assert!(config.mount_tmpfs);
        let filter = config.syscall_filter.unwrap();
        assert_eq!(filter.default_action, SyscallFilterAction::Allow);
        let rules: HashMap<_, _> = filter.rules.into_iter().collect();
        assert!(!rules.contains_key("fork"));
        assert!(!rules.contains_key("vfork"));
        assert!(config.env.contains(&("foo".to_string(), "bar".to_string())));
        assert_eq!(config.stdin, Some("/dev/null".into()));
        assert_eq!(config.stdout, Some("/dev/null".into()));
        assert_eq!(config.stderr, Some("/dev/null".into()));
        assert_eq!(config.executable, boxdir.join("foo"));
        assert_eq!(config.args, vec!["bar", "baz"]);
    }
}
