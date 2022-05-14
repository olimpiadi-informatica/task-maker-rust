//! This mod contains the sandbox-related code. It interfaces with tabox creating the sandbox setup
//! (directories and configuration) for an execution.

use std::collections::HashMap;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Error};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use tabox::configuration::SandboxConfiguration;
use tabox::result::SandboxExecutionResult;
use tabox::syscall_filter::SyscallFilter;
use tempdir::TempDir;

use task_maker_dag::*;
use task_maker_store::*;

use crate::detect_exe::detect_exe;
use crate::sandbox_runner::SandboxRunner;

/// The list of all the system-wide readable directories inside the sandbox.
pub const READABLE_DIRS: &[&str] = &[
    "/lib",
    "/lib64",
    "/usr",
    "/bin",
    "/opt",
    // update-alternatives stuff, sometimes the executables are symlinked here
    "/etc/alternatives/",
    "/var/lib/dpkg/alternatives/",
    // required by texlive on Ubuntu
    "/var/lib/texmf/",
];

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

/// Internals of the sandbox.
#[derive(Debug)]
struct SandboxData {
    /// Handle to the temporary directory, will be deleted on drop. It's always Some(_) except
    /// inside `Drop`.
    boxdir: Option<TempDir>,
    /// Execution to run.
    execution: Execution,
    /// Whether to keep the sandbox after exit.
    keep_sandbox: bool,
    /// Directory where the FIFO pipes are stored.
    fifo_dir: Option<PathBuf>,
    /// The PID of the sandbox process, zero if not available or not spawned yet.
    box_pid: Arc<AtomicU32>,
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
}

impl Sandbox {
    /// Make a new sandbox for the specified execution, copying all the required files. To start the
    /// sandbox call `run`.
    pub fn new(
        sandboxes_dir: &Path,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreHandle>,
        fifo_dir: Option<PathBuf>,
    ) -> Result<Sandbox, Error> {
        std::fs::create_dir_all(sandboxes_dir).with_context(|| {
            format!(
                "Failed to create sandbox directory at {}",
                sandboxes_dir.display()
            )
        })?;
        let boxdir = TempDir::new_in(sandboxes_dir, "box")
            .context("Failed to create sandbox temporary directory")?;
        Sandbox::setup(boxdir.path(), execution, dep_keys).context("Sandbox setup failed")?;
        Ok(Sandbox {
            data: Arc::new(Mutex::new(SandboxData {
                boxdir: Some(boxdir),
                execution: execution.clone(),
                keep_sandbox: false,
                fifo_dir,
                box_pid: Arc::new(AtomicU32::new(0)),
            })),
        })
    }

    /// Starts the sandbox and blocks the thread until the sandbox exits.
    pub fn run(&self, runner: &dyn SandboxRunner) -> Result<SandboxResult, Error> {
        let mut config = SandboxConfiguration::default();
        let (boxdir, pid, keep, cmd) = {
            let data = self.data.lock().unwrap();
            (
                data.path().to_owned(),
                data.box_pid.clone(),
                data.keep_sandbox,
                self.build_command(
                    data.path(),
                    &data.execution,
                    &mut config,
                    data.fifo_dir.clone(),
                ),
            )
        };
        trace!("Running sandbox at {:?}", boxdir);

        if let Err(e) = cmd {
            return Ok(SandboxResult::Failed {
                error: e.to_string(),
            });
        }
        trace!("Sandbox configuration: {:#?}", config);

        let raw_result = runner.run(config.build(), pid);
        if keep {
            let target = boxdir.join("result.txt");
            std::fs::write(&target, format!("{:#?}", raw_result))
                .with_context(|| format!("Failed to write {}", target.display()))?;
        }

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
                signal: Some((
                    s as u32,
                    res.status.signal_name().unwrap_or_else(|| "unknown".into()),
                )),
                resources,
                was_killed: false,
            }),
            Killed => Ok(SandboxResult::Success {
                exit_status: 1,
                signal: Some((9, "Killed by sandbox".into())),
                resources,
                was_killed: true,
            }),
        }
    }

    /// Tell the sandbox process to kill the underlying process, this will make `run` terminate more
    /// quickly.
    pub fn kill(&self) {
        let (path, box_pid) = {
            let data = self.data.lock().unwrap();
            (data.path().to_path_buf(), data.box_pid.clone())
        };
        let path = path.display();
        let mut pid = 0;
        // Race condition here: the sandbox may have been created but the process is not spawned
        // yet. This means that the PID is not available yet but will be soon.
        for _ in 0..5 {
            // try getting the PID
            pid = box_pid.load(Ordering::SeqCst);
            if pid != 0 {
                break;
            } else {
                // if the PID has not been set yet try again in few milliseconds
                warn!("Sandbox at {} has no known pid... waiting", path);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        // if after many tries the PID has not been set lose hope and don't kill the sandbox.
        if pid == 0 {
            warn!("Cannot kill sandbox at {} since the pid is unknown", path);
            return;
        }
        info!("Sandbox at {:?} (pid {}) will be killed", path, pid);
        if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
            warn!("Cannot kill sandbox at {} (pid {}): {:?}", path, pid, e);
        }
    }

    /// Make the sandbox persistent, the sandbox directory won't be deleted after the execution.
    pub fn keep(&mut self) -> Result<(), Error> {
        let mut data = self.data.lock().unwrap();
        let path = data
            .boxdir
            .as_ref()
            .context("Box dir has gone")?
            .path()
            .to_owned();
        debug!("Keeping sandbox at {:?}", path);
        data.keep_sandbox = true;
        let serialized = serde_json::to_string_pretty(&data.execution)
            .context("Failed to serialize execution")?;
        std::fs::write(path.join("info.json"), serialized)
            .context("Cannot write execution info inside sandbox")?;
        let mut config = SandboxConfiguration::default();
        if let Ok(()) =
            self.build_command(&path, &data.execution, &mut config, data.fifo_dir.clone())
        {
            std::fs::write(path.join("tabox.txt"), format!("{:#?}\n", config))
                .context("Cannot write command info inside sandbox")?;
        }
        Ok(())
    }

    /// Path of the file where the standard output is written to (in the host).
    pub fn stdout_path(&self) -> PathBuf {
        let data = self.data.lock().unwrap();
        let sandbox_root = data.path();
        if let Some(path) = &data.execution.stdout_redirect_path {
            sandbox_root.join(path)
        } else {
            sandbox_root.join("stdout")
        }
    }

    /// Path of the file where the standard error is written to (in the host).
    pub fn stderr_path(&self) -> PathBuf {
        let data = self.data.lock().unwrap();
        let sandbox_root = data.path();
        if let Some(path) = &data.execution.stderr_redirect_path {
            sandbox_root.join(path)
        } else {
            sandbox_root.join("stderr")
        }
    }

    /// Path of the file where that output file is written to (in the host).
    pub fn output_path(&self, output: &Path) -> PathBuf {
        self.data.lock().unwrap().path().join("box").join(output)
    }

    /// Find the path in the host corresponding to the path in the sandbox provided.
    fn sandbox_to_host_path(
        &self,
        path_in_sandbox: &Path,
        boxdir: &Path,
        fifo_dir: Option<&Path>,
    ) -> PathBuf {
        if let Some(fifo_dir) = fifo_dir {
            if let Ok(path) = path_in_sandbox.strip_prefix(FIFO_SANDBOX_DIR) {
                return fifo_dir.join(path);
            }
        }
        match path_in_sandbox.strip_prefix("/") {
            // Absolute path -> go the box root
            Ok(path) => boxdir.join(path),
            // Relative path -> go to the /box directory
            Err(_) => self.box_root(boxdir).join(path_in_sandbox),
        }
    }

    /// Directory to use inside the sandbox as the root for the evaluation.
    ///
    /// Due to a limitation of `tabox`, under macos the sandbox is not able to mount the directories
    /// well (the bind-mounts are not available), so `/box` cannot be emulated easily. To workaround
    /// this limitation, only under macos the original path is kept. This leaks some information
    /// about the host, but since the sandbox is pretty fake anyway this is not really a problem.
    #[allow(unused_variables)]
    fn box_root(&self, boxdir: &Path) -> PathBuf {
        #[cfg(not(target_os = "macos"))]
        {
            PathBuf::from("/box")
        }
        #[cfg(target_os = "macos")]
        {
            boxdir.join("box")
        }
    }

    /// Build the configuration of the tabox sandbox.
    fn build_command(
        &self,
        boxdir: &Path,
        execution: &Execution,
        config: &mut SandboxConfiguration,
        fifo_dir: Option<PathBuf>,
    ) -> Result<(), Error> {
        let box_root = self.box_root(boxdir);
        config.working_directory(&box_root);
        // the box directory must be writable otherwise the output files cannot be written
        config.mount(boxdir.join("box"), &box_root, true);
        config.env("PATH", std::env::var("PATH").unwrap_or_default());
        if let Some(path) = &execution.stdin_redirect_path {
            config.stdin(self.sandbox_to_host_path(path, boxdir, fifo_dir.as_deref()));
        } else if execution.stdin.is_some() {
            config.stdin(boxdir.join("stdin"));
        } else {
            config.stdin("/dev/null");
        }
        if let Some(path) = &execution.stdout_redirect_path {
            config.stdout(self.sandbox_to_host_path(path, boxdir, fifo_dir.as_deref()));
        } else if execution.stdout.is_some() {
            config.stdout(boxdir.join("stdout"));
        } else {
            config.stdout("/dev/null");
        }
        if let Some(path) = &execution.stderr_redirect_path {
            config.stderr(self.sandbox_to_host_path(path, boxdir, fifo_dir.as_deref()));
        } else if execution.stderr.is_some() {
            config.stderr(boxdir.join("stderr"));
        } else {
            config.stderr("/dev/null");
        }
        for key in execution.copy_env.iter() {
            if let Ok(value) = std::env::var(key) {
                config.env(key, value);
            }
        }
        for (key, value) in execution.env.iter() {
            config.env(key, value);
        }

        let cpu_limit = match (execution.limits.cpu_time, execution.limits.sys_time) {
            (Some(cpu), Some(sys)) => Some(cpu + sys),
            (Some(cpu), None) => Some(cpu),
            (None, Some(sys)) => Some(sys),
            (None, None) => None,
        };
        if let Some(cpu) = cpu_limit {
            let cpu = cpu + execution.config().extra_time;
            config.time_limit(cpu.ceil() as u64);
        }
        if let Some(wall) = execution.limits.wall_time {
            let wall = wall + execution.config().extra_time;
            config.wall_time_limit(wall.ceil() as u64);
        }
        if let Some(mem) = execution.limits.memory {
            config.memory_limit(mem * 1024);
        }
        if let Some(stack) = execution.limits.stack {
            config.stack_limit(stack * 1024);
        }
        config.syscall_filter(SyscallFilter::build(
            execution.limits.allow_multiprocess,
            !execution.limits.read_only,
        ));
        // has to be writable for mounting stuff in it
        config.mount(boxdir.join("etc"), "/etc", true);
        if let Some(path) = fifo_dir {
            // allow access knowing the path but prevent listing the dir content
            Sandbox::set_permissions(&path, 0o111)
                .with_context(|| format!("Failed to chmod 111 {}", path.display()))?;
            config.mount(path, box_root.join(FIFO_SANDBOX_DIR), false);
        }
        for dir in READABLE_DIRS {
            if Path::new(dir).is_dir() {
                config.mount(dir, dir, false);
            }
        }
        for dir in &execution.limits.extra_readable_dirs {
            if dir.is_dir() {
                config.mount(dir, dir, false);
            }
        }
        if execution.limits.mount_tmpfs {
            config.mount_tmpfs(true);
        }
        if execution.limits.mount_proc {
            config.mount_proc(true);
        }
        match &execution.command {
            ExecutionCommand::System(cmd) => {
                if let Ok(cmd) = which::which(cmd) {
                    config.executable(cmd);
                } else {
                    bail!("Executable {:?} not found", cmd);
                }
            }
            ExecutionCommand::Local(cmd) => {
                let host_cmd = boxdir.join("box").join(cmd);
                self.validate_local_executable(&host_cmd).with_context(|| {
                    format!(
                        "Local sandbox executable validation failed: {}",
                        &host_cmd.display()
                    )
                })?;
                config.executable(box_root.join(cmd));
            }
        };
        for arg in execution.args.iter() {
            config.arg(arg);
        }
        // drop root privileges in the sandbox
        config.uid(1000);
        config.gid(1000);
        Ok(())
    }

    /// Setup the sandbox directory with all the files required for the execution.
    fn setup<P: AsRef<Path>>(
        box_dir: P,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreHandle>,
    ) -> Result<(), Error> {
        let box_dir = box_dir.as_ref();
        trace!(
            "Setting up sandbox at {:?} for '{}'",
            box_dir,
            execution.description
        );
        Self::create_sandbox_dir(box_dir, "box")?;
        // put /etc/passwd inside the sandbox
        Self::create_sandbox_dir(box_dir, "etc")?;
        std::fs::write(
            box_dir.join("etc").join("passwd"),
            "root::0:0::/:/bin/sh\n\
            nobody::1000:1000::/:/bin/sh\n",
        )
        .with_context(|| {
            format!(
                "Failed to write /etc/passwd in the sandbox {}",
                box_dir.display()
            )
        })?;

        if let Some(stdin) = execution.stdin {
            Sandbox::write_sandbox_file(
                &box_dir.join("stdin"),
                dep_keys.get(&stdin).context("stdin not provided")?.path(),
                false,
            )?;
        }
        if execution.stdout.is_some() {
            Sandbox::touch_file(&box_dir.join("stdout"), 0o600)?;
        }
        if execution.stderr.is_some() {
            Sandbox::touch_file(&box_dir.join("stderr"), 0o600)?;
        }
        for (path, input) in execution.inputs.iter() {
            Sandbox::write_sandbox_file(
                &box_dir.join("box").join(&path),
                dep_keys
                    .get(&input.file)
                    .context("file not provided")?
                    .path(),
                input.executable,
            )?;
        }
        for path in execution.outputs.keys() {
            Sandbox::touch_file(&box_dir.join("box").join(&path), 0o600)?;
        }
        // remove the write bit on the box folder
        if execution.limits.read_only {
            Sandbox::set_permissions(&box_dir.join("box"), 0o500)?;
        }
        trace!("Sandbox at {:?} ready!", box_dir);
        Ok(())
    }

    /// Create a directory inside the sandbox.
    fn create_sandbox_dir<P: AsRef<Path>>(box_dir: &Path, path: P) -> Result<(), Error> {
        let target = box_dir.join(path.as_ref());
        std::fs::create_dir_all(&target)
            .with_context(|| format!("Failed to create sandbox directory: {}", target.display()))
    }

    /// Put a file inside the sandbox, creating the directories if needed and making it executable
    /// if needed.
    ///
    /// The file will have the most restrictive permissions possible:
    /// - `r--------` (0o400) if not executable.
    /// - `r-x------` (0o500) if executable.
    fn write_sandbox_file(dest: &Path, source: &Path, executable: bool) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().context("Invalid destination path")?)
            .with_context(|| format!("Failed to create parent directory of {}", dest.display()))?;
        // First try to hardlink the file to the destination, this is faster and less prone to race
        // conditions. If another thread forks while copying the executable (for example spawning a
        // sandbox of another worker) the file descriptor won't be closed while this sandbox tries
        // to exec the process, failing with "Text file busy".
        if std::fs::hard_link(source, dest).is_err() {
            std::fs::copy(source, dest).with_context(|| {
                format!("Failed to copy {} -> {}", source.display(), dest.display())
            })?;
        }
        if executable {
            Sandbox::set_permissions(dest, 0o500)?;
        } else {
            Sandbox::set_permissions(dest, 0o400)?;
        }
        Ok(())
    }

    /// Create an empty file inside the sandbox and chmod-it.
    fn touch_file(dest: &Path, mode: u32) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().context("Invalid file path")?)
            .with_context(|| format!("Failed to create parent directory of {}", dest.display()))?;
        std::fs::File::create(dest)
            .with_context(|| format!("Failed to create {}", dest.display()))?;
        Self::set_permissions(dest, mode)?;
        Ok(())
    }

    fn set_permissions(dest: &Path, perm: u32) -> Result<(), Error> {
        let permissions = Permissions::from_mode(perm);
        std::fs::set_permissions(dest, permissions)
            .with_context(|| format!("Failed to chmod {:03o} {}", perm, dest.display()))?;
        Ok(())
    }

    /// Check that a path is a valid local executable.
    ///
    /// To be a valid executable the file must be _a file_ and should be in a recognized executable
    /// format.
    fn validate_local_executable<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let path = path.as_ref();
        if !path.is_file() {
            bail!("Executable is not a file");
        }
        let exe = detect_exe(path).context("Failed to detect sandbox executable")?;
        if exe.is_none() {
            bail!("Invalid executable, missing shebang?");
        }
        Ok(())
    }
}

impl SandboxData {
    fn path(&self) -> &Path {
        // this unwrap is safe since only `Drop` will remove the boxdir
        self.boxdir.as_ref().expect("boxdir is gone").path()
    }
}

impl Drop for SandboxData {
    fn drop(&mut self) {
        if self.keep_sandbox {
            // this will unwrap the directory, dropping the `TempDir` without deleting the directory
            self.boxdir.take().map(TempDir::into_path);
        } else if Sandbox::set_permissions(&self.path().join("box"), 0o700).is_err() {
            warn!("Cannot 'chmod 700' the sandbox directory");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use tabox::configuration::{DirectoryMount, SandboxConfiguration};
    use tabox::syscall_filter::SyscallFilterAction;

    use task_maker_dag::{Execution, ExecutionCommand};

    use crate::sandbox::Sandbox;
    use crate::ErrorSandboxRunner;

    #[test]
    fn test_remove_sandbox_on_drop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let mut exec = Execution::new("test", ExecutionCommand::system("true"));
        exec.output("fooo");
        exec.limits_mut().read_only(true);
        let sandbox = Sandbox::new(tmpdir.path(), &exec, &HashMap::new(), None).unwrap();
        let outfile = sandbox.output_path(Path::new("fooo"));
        if let Err(e) = sandbox.run(&ErrorSandboxRunner::default()) {
            assert!(e.to_string().contains("Nope"));
        } else {
            panic!("Sandbox not called");
        }
        drop(sandbox);
        assert!(!outfile.exists());
        assert!(!outfile.parent().unwrap().exists()); // the box/ dir
        assert!(!outfile.parent().unwrap().parent().unwrap().exists()); // the sandbox dir
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_command_args() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let mut exec = Execution::new("test", ExecutionCommand::system("/bin/sh"));
        exec.args(vec!["bar", "baz"]);
        exec.limits_mut()
            .sys_time(1.0)
            .cpu_time(2.6)
            .wall_time(10.0)
            .mount_tmpfs(true)
            .add_extra_readable_dir("/home")
            .allow_multiprocess()
            .memory(1234);
        exec.env("foo", "bar");
        let sandbox = Sandbox::new(tmpdir.path(), &exec, &HashMap::new(), None).unwrap();
        let mut config = SandboxConfiguration::default();
        sandbox
            .build_command(tmpdir.path(), &exec, &mut config, None)
            .unwrap();
        let extra_time = exec.config().extra_time;
        let total_time = (1.0 + 2.6 + extra_time).ceil() as u64;
        let wall_time = (10.0 + extra_time).ceil() as u64;
        assert_eq!(config.working_directory, Path::new("/box"));
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
        assert_eq!(config.executable, Path::new("/bin/sh"));
        assert_eq!(config.args, vec!["bar", "baz"]);
    }
}
