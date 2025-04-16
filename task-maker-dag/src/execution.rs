use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::file::*;
use crate::ExecutionDAGConfig;

/// The identifier of an execution, it's globally unique and it identifies an execution only during
/// a single evaluation.
pub type ExecutionUuid = Uuid;

/// The identifier of a worker, it's globally unique and identifies the worker during a single
/// connection. It is used to associate the jobs to the workers which runs the executions. The
/// underlying executor may provide more information about a worker using this id.
pub type WorkerUuid = Uuid;

/// Type of the callback called when an [`Execution`](struct.Execution.html) starts.
pub type OnStartCallback = Box<dyn FnOnce(WorkerUuid) -> Result<(), Error> + 'static>;

/// Type of the callback called when an [`Execution`](struct.Execution.html) ends.
pub type OnDoneCallback = Box<dyn FnOnce(ExecutionResult) -> Result<(), Error> + 'static>;

/// Type of the callback called when an [`Execution`](struct.Execution.html) is skipped.
pub type OnSkipCallback = Box<dyn FnOnce() -> Result<(), Error> + 'static>;

/// Type of the priority value of an `Execution`.
pub type Priority = i64;

/// Type of the priority value of a DAG.
pub type DagPriority = Priority;

/// DAG-priority of executions that should run very soon, independently of their DAG, for example
/// executions that became available after a previous execution finished, or retries.
pub const HIGH_PRIORITY: DagPriority = 1_000_000_000_000;

/// A tag on an `Execution`. Can be used to classify the executions into groups and refer to them,
/// for example for splitting the cache scopes.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ExecutionTag {
    /// The name of the tag.
    pub name: String,
}

/// Command of an [`Execution`](struct.Execution.html) to execute.
///
/// There is a distinction between a `System` command, which has to be searched in the `PATH`
/// env var, and a `Local` command, which is relative to the sandbox directory.
///
/// ```
/// use task_maker_dag::ExecutionCommand;
///
/// let sys_cmd = ExecutionCommand::system("/usr/bin/env");
/// let sys_cmd = ExecutionCommand::system("env"); // looking at $PATH
/// let local_cmd = ExecutionCommand::local("generator"); // local to the cwd of the sandbox
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExecutionCommand {
    /// A system command, the workers will search in their `$PATH` for the executable if it's not
    /// absolute.
    System(PathBuf),
    /// A command relative to the sandbox directory, not to be searched in the `$PATH`.
    Local(PathBuf),
}

/// An input file of an [`Execution`](struct.Execution.html), can be marked as executable if it has
/// to be run inside the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct ExecutionInput {
    /// Uuid of the file.
    pub file: FileUuid,
    /// Whether this file should be marked as executable.
    pub executable: bool,
}

/// The callbacks to be called when an event of an execution occurs.
#[derive(Default)]
pub struct ExecutionCallbacks {
    /// The callbacks called when the execution starts.
    pub on_start: Vec<OnStartCallback>,
    /// The callbacks called when the execution has completed.
    pub on_done: Vec<OnDoneCallback>,
    /// The callbacks called when the execution has been skipped.
    pub on_skip: Vec<OnSkipCallback>,
}

/// An [`Execution`](struct.Execution.html) is a process that will be executed by a worker inside a
/// sandbox. The sandbox will limit the execution of the process (e.g. killing it after a time limit
/// occurs, or preventing it from reading/writing files).
///
/// Inside the sandbox the process will execute a `command` with the specified arguments, passing an
/// optional standard input and capturing optionally the `stdout` and `stderr`. A list of files is
/// also inserted inside the sandbox for the process to read and a list of files is captured as
/// output.
///
/// The execution will also specify the limits on the process.
///
/// ```
/// use task_maker_dag::{Execution, ExecutionCommand, File, ExecutionDAG, ExecutionLimits};
///
/// // provide an input file
/// let stdin = File::new("random file");
///
/// // first execution reading stdin, outputting to stdout, with 2s cpu limit, 3s wall limit and
/// // 1MiB of memory.
/// let mut exec = Execution::new("some hard work", ExecutionCommand::local("worker"));
/// exec.stdin(&stdin);
/// let stdout = exec.stdout();
/// exec.limits_mut().cpu_time(2.0).wall_time(3.0).memory(1024);
///
/// // second execution, will run after the first because it depends on its output, only if the
/// // first is successful. Will take the stdout of the first as a file input and will capture the
/// // stdout.
/// let mut exec2 = Execution::new("some other work", ExecutionCommand::local("worker2"));
/// exec2.input(&stdout, "data.txt", false); // put the stdout of exec inside data.txt of exec2
/// let result = exec2.stdout();
///
/// // add the executions and files to the dag
/// let mut dag = ExecutionDAG::new();
/// dag.add_execution(exec);
/// dag.add_execution(exec2);
/// dag.provide_file(stdin, "/dev/null"); // point this to a real file!!
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    /// Uuid of the execution.
    pub uuid: ExecutionUuid,
    /// Description of the execution.
    pub description: String,
    /// Which command to execute.
    pub command: ExecutionCommand,
    /// The list of command line arguments.
    pub args: Vec<String>,

    /// Optional standard input to pass to the program.
    pub stdin: Option<FileUuid>,
    /// Optional standard output to capture.
    pub stdout: Option<File>,
    /// Optional standard error to capture.
    pub stderr: Option<File>,
    /// When not `None`, ask the sandbox to capture that many bytes from the standard output.
    pub capture_stdout: Option<usize>,
    /// When not `None`, ask the sandbox to capture that many bytes from the standard error.
    pub capture_stderr: Option<usize>,
    /// List of input files that should be put inside the sandbox.
    pub inputs: HashMap<PathBuf, ExecutionInput>,
    /// List of the output files that should be capture from the sandbox.
    pub outputs: HashMap<PathBuf, File>,
    /// When set, the standard input is redirected from this file. The path is relative to the
    /// sandbox root.
    ///
    /// This is incompatible with `stdin`. Dependencies can still be achieved by providing the file
    /// as an "input".
    pub stdin_redirect_path: Option<PathBuf>,
    /// When set, the standard output is redirected to this file. The path is relative to the
    /// sandbox root.
    pub stdout_redirect_path: Option<PathBuf>,
    /// When set, the standard error is redirected to this file. The path is relative to the
    /// sandbox root.
    pub stderr_redirect_path: Option<PathBuf>,

    /// Environment variables to set.
    pub env: HashMap<String, String>,
    /// Environment variables to copy from the sandbox host.
    pub copy_env: Vec<String>,

    /// Limits on the execution.
    pub limits: ExecutionLimits,

    /// The configuration of the underlying DAG. Will be overwritten by
    /// `ExecutionDAG.add_execution`.
    pub(crate) config: ExecutionDAGConfig,

    /// The tag associated with this execution.
    pub tag: Option<ExecutionTag>,
    /// A priority index for this execution. Higher values correspond to higher priorities. The
    /// priority order is followed only between ready executions, i.e. a lower priority one can be
    /// executed before if its dependencies are ready earlier.
    pub priority: Priority,
}

/// Limits on an [`Execution`](struct.Execution.html). On some worker platforms some of the fields
/// may not be supported or may be less accurate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionLimits {
    /// Limit on the userspace cpu time of the process, in seconds.
    pub cpu_time: Option<f64>,
    /// Limit on the kernels pace cpu time of the process, in seconds.
    pub sys_time: Option<f64>,
    /// Limit on the total time of execution, in seconds. This will include the io-wait time and
    /// other non-cpu times.
    pub wall_time: Option<f64>,
    /// Limit on the number of KiB the process can use in any moment. This can be page-aligned by
    /// the sandbox.
    pub memory: Option<u64>,
    /// Allow multiple processes (i.e. forking/threads).
    pub allow_multiprocess: bool,
    /// Limit on the number of file descriptors the process can keep open.
    pub nofile: Option<u32>,
    /// Maximum size of the files (in bytes) the process can write/create.
    pub fsize: Option<u64>,
    /// RLIMIT_MEMLOCK
    pub memlock: Option<u64>,
    /// Limit on the stack size for the process in KiB.
    pub stack: Option<u64>,
    /// Whether the process in the sandbox is not allowed to create new files inside the sandbox.
    pub read_only: bool,
    /// Whether the process in the sandbox can use `/dev/null` and `/tmp`.
    pub mount_tmpfs: bool,
    /// Whether the process in the sandbox can use `/proc`.
    pub mount_proc: bool,
    /// Extra directory that can be read inside the sandbox.
    pub extra_readable_dirs: Vec<PathBuf>,
}

/// Status of a completed [`Execution`](struct.Execution.html).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// The program exited with status 0 within the limits.
    Success,
    /// The program exited with a non-zero status code, which is attached.
    ReturnCode(u32),
    /// The program stopped due to a signal, the number and the name of the signal are attached.
    Signal(u32, String),
    /// The program hasn't exited within the time limit constraint.
    TimeLimitExceeded,
    /// The program hasn't exited within the sys time limit constraint.
    SysTimeLimitExceeded,
    /// The program hasn't exited within the wall time limit constraint.
    WallTimeLimitExceeded,
    /// The program has exceeded the memory limit.
    MemoryLimitExceeded,
    /// The sandbox failed to execute the program with the attached error message.
    InternalError(String),
}

/// Resources used during the execution, note that on some platform these values may not be
/// accurate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExecutionResourcesUsage {
    /// Number of seconds the process used in user space.
    pub cpu_time: f64,
    /// Number of seconds the process used in kernel space.
    pub sys_time: f64,
    /// Number of seconds from the start to the end of the process.
    pub wall_time: f64,
    /// Number of KiB used _at most_ by the process.
    pub memory: u64,
}

/// The result of an [`Execution`](struct.Execution.html).
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionResult {
    /// Status of the completed execution.
    pub status: ExecutionStatus,
    /// Whether the execution has been killed by the sandbox.
    pub was_killed: bool,
    /// Whether the execution result come from the cache.
    pub was_cached: bool,
    /// Resources used by the execution.
    pub resources: ExecutionResourcesUsage,
    /// Captured standard output of the execution, if the capture was requested.
    pub stdout: Option<Vec<u8>>,
    /// Captured standard error of the execution, if the capture was requested.
    pub stderr: Option<Vec<u8>>,
}

impl ExecutionLimits {
    /// Make an empty limits where all the limits are disabled. You may want to
    /// use `default()` instead of this
    pub fn unrestricted() -> ExecutionLimits {
        ExecutionLimits {
            cpu_time: None,
            sys_time: None,
            wall_time: None,
            memory: None,
            allow_multiprocess: true,
            nofile: None,
            fsize: None,
            memlock: None,
            stack: None,
            read_only: false,
            mount_tmpfs: true,
            mount_proc: true,
            extra_readable_dirs: Vec::new(),
        }
    }

    /// Set the cpu time limit in seconds.
    pub fn cpu_time(&mut self, limit: f64) -> &mut Self {
        self.cpu_time = Some(limit);
        self
    }

    /// Set the sys time limit in seconds.
    pub fn sys_time(&mut self, limit: f64) -> &mut Self {
        self.sys_time = Some(limit);
        self
    }

    /// Set the wall time limit in seconds.
    pub fn wall_time(&mut self, limit: f64) -> &mut Self {
        self.wall_time = Some(limit);
        self
    }

    /// Set the memory limit in KiB.
    pub fn memory(&mut self, limit: u64) -> &mut Self {
        self.memory = Some(limit);
        self
    }

    /// Allow multiple processes.
    pub fn allow_multiprocess(&mut self) -> &mut Self {
        self.allow_multiprocess = true;
        self
    }

    /// Block multiple processes.
    pub fn block_multiprocess(&mut self) -> &mut Self {
        self.allow_multiprocess = false;
        self
    }

    /// Set the maximum number of opened file descriptors.
    pub fn nofile(&mut self, limit: u32) -> &mut Self {
        self.nofile = Some(limit);
        self
    }

    /// Set the maximum size of the files (in bytes) the process can write/create.
    pub fn fsize(&mut self, limit: u64) -> &mut Self {
        self.fsize = Some(limit);
        self
    }

    /// Set the memlock limit.
    pub fn memlock(&mut self, limit: u64) -> &mut Self {
        self.memlock = Some(limit);
        self
    }

    /// Set the stack limit.
    pub fn stack(&mut self, limit: u64) -> &mut Self {
        self.stack = Some(limit);
        self
    }

    /// Set whether the process in the sandbox is not allowed to create new files inside the
    /// sandbox.
    pub fn read_only(&mut self, read_only: bool) -> &mut Self {
        self.read_only = read_only;
        self
    }

    /// Set whether the process in the sandbox can use `/dev/null` and `/tmp`.
    pub fn mount_tmpfs(&mut self, mount_tmpfs: bool) -> &mut Self {
        self.mount_tmpfs = mount_tmpfs;
        self
    }

    /// Set whether the process in the sandbox can use `/proc`.
    pub fn mount_proc(&mut self, mount_proc: bool) -> &mut Self {
        self.mount_proc = mount_proc;
        self
    }

    /// Add a directory to the list of additional readable directories in the sandbox.
    pub fn add_extra_readable_dir<P: Into<PathBuf>>(&mut self, dir: P) -> &mut Self {
        self.extra_readable_dirs.push(dir.into());
        self
    }
}

impl Default for ExecutionLimits {
    /// Default sane values for the execution limits, the limits listed here
    /// should be safe enough for most of the executions.
    fn default() -> Self {
        ExecutionLimits {
            cpu_time: None,
            sys_time: None,
            wall_time: None,
            memory: None,
            allow_multiprocess: false,
            nofile: None,
            fsize: Some(1024u64.pow(3)),
            memlock: None,
            stack: None,
            read_only: true,
            mount_tmpfs: false,
            mount_proc: false,
            extra_readable_dirs: Vec::new(),
        }
    }
}

impl ExecutionStatus {
    /// Returns true only if it is `ExecutionStatus::Success`.
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionStatus::Success)
    }

    /// Returns true only if it is `ExecutionStatus::InternalError(_)`.
    pub fn is_internal_error(&self) -> bool {
        matches!(self, ExecutionStatus::InternalError(_))
    }

    /// Build a new `ExecutionStatus` as an `InternalError`.
    pub fn internal_error<S: Into<String>>(error: S) -> Self {
        Self::InternalError(error.into())
    }
}

impl ExecutionCommand {
    /// Make a new `ExecutionCommand::System`.
    pub fn system<P: Into<PathBuf>>(path: P) -> ExecutionCommand {
        ExecutionCommand::System(path.into())
    }

    /// Make a new `ExecutionCommand::Local`.
    pub fn local<P: Into<PathBuf>>(path: P) -> ExecutionCommand {
        ExecutionCommand::Local(path.into())
    }
}

impl Execution {
    /// Create a basic [`Execution`](struct.Execution.html) with the default limits.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// ```
    pub fn new<S: Into<String>>(description: S, command: ExecutionCommand) -> Execution {
        Execution {
            uuid: Uuid::new_v4(),

            description: description.into(),
            command,
            args: vec![],

            stdin: None,
            stdout: None,
            stderr: None,
            capture_stdout: None,
            capture_stderr: None,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            stdin_redirect_path: None,
            stdout_redirect_path: None,
            stderr_redirect_path: None,

            env: HashMap::new(),
            copy_env: Vec::new(),

            limits: ExecutionLimits::default(),

            config: ExecutionDAGConfig::new(),

            tag: None,
            priority: Priority::default(),
        }
    }

    /// List of all the [File](struct.File.html) dependencies of the execution, including `stdin`.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = File::new("random file");
    /// let uuid = file.uuid;
    /// exec.stdin(file);
    /// assert_eq!(exec.dependencies(), vec![uuid]);
    /// ```
    pub fn dependencies(&self) -> Vec<FileUuid> {
        let mut deps = vec![];
        if let Some(stdin) = self.stdin {
            deps.push(stdin);
        }
        for input in self.inputs.values() {
            deps.push(input.file);
        }
        deps
    }

    /// List of all the [File](struct.File.html) produced by the execution, including `stdout` and
    /// `stderr`.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = exec.stdout();
    /// let uuid = file.uuid;
    /// assert_eq!(exec.outputs(), vec![uuid]);
    /// ```
    pub fn outputs(&self) -> Vec<FileUuid> {
        let mut outs = vec![];
        if let Some(stdout) = &self.stdout {
            outs.push(stdout.uuid);
        }
        if let Some(stderr) = &self.stderr {
            outs.push(stderr.uuid);
        }
        for output in self.outputs.values() {
            outs.push(output.uuid);
        }
        outs
    }

    /// Sets the command line arguments of the execution. Calling again this method will overwrite
    /// the previous values.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("test execution", ExecutionCommand::local("foo"));
    /// exec.args(vec!["test", "args"]);
    /// ```
    pub fn args<S: Into<String>, I: IntoIterator<Item = S>>(&mut self, args: I) -> &mut Self {
        self.args = args.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Bind the standard input to the specified file. Calling again this method will overwrite the
    /// previous value.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = File::new("random file");
    /// let uuid = file.uuid;
    /// exec.stdin(file);
    /// assert_eq!(exec.stdin, Some(uuid));
    /// ```
    pub fn stdin<F: Into<FileUuid>>(&mut self, stdin: F) -> &mut Self {
        assert!(self.stdin_redirect_path.is_none());
        self.stdin = Some(stdin.into());
        self
    }

    /// Set the stdin redirection path.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    /// use std::path::PathBuf;
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.stdin_redirect_path("/dev/urandom");
    /// assert_eq!(exec.stdin_redirect_path.unwrap(), PathBuf::from("/dev/urandom"));
    /// ```
    pub fn stdin_redirect_path<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        assert!(self.stdin.is_none());
        self.stdin_redirect_path = Some(path.into());
        self
    }

    /// Handle to the standard output of the execution. This should be called at least once before
    /// the evaluation starts in order to track the file. Calling this method more than once will
    /// return the same value.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = exec.stdout();
    /// assert_eq!(exec.stdout, Some(file));
    /// ```
    pub fn stdout(&mut self) -> File {
        if self.stdout.is_none() {
            let file = File::new(format!("Stdout of '{}'", self.description));
            self.stdout = Some(file);
        }
        self.stdout.clone().unwrap()
    }

    /// Set the stdout redirection path.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    /// use std::path::PathBuf;
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.stdout_redirect_path("output.txt");
    /// assert_eq!(exec.stdout_redirect_path.unwrap(), PathBuf::from("output.txt"));
    /// ```
    pub fn stdout_redirect_path<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        self.stdout_redirect_path = Some(path.into());
        self
    }

    /// Handle to the standard error of the execution. This should be called at least once before
    /// the evaluation starts in order to track the file. Calling this method more than once will
    /// return the same value.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = exec.stderr();
    /// assert_eq!(exec.stderr, Some(file));
    /// ```
    pub fn stderr(&mut self) -> File {
        if self.stderr.is_none() {
            let file = File::new(format!("Stderr of '{}'", self.description));
            self.stderr = Some(file);
        }
        self.stderr.clone().unwrap()
    }

    /// Set the stderr redirection path.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    /// use std::path::PathBuf;
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.stderr_redirect_path("error.txt");
    /// assert_eq!(exec.stderr_redirect_path.unwrap(), PathBuf::from("error.txt"));
    /// ```
    pub fn stderr_redirect_path<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        self.stderr_redirect_path = Some(path.into());
        self
    }

    /// Tell the executor to include the first `count` bytes of the standard output in the result.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.capture_stdout(1234);
    /// assert_eq!(exec.capture_stdout, Some(1234));
    /// ```
    pub fn capture_stdout(&mut self, count: usize) -> &mut Self {
        self.stdout(); // make sure stdout is captured
        self.capture_stdout = Some(count);
        self
    }

    /// Tell the executor to include the first `count` bytes of the standard error in the result.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.capture_stderr(1234);
    /// assert_eq!(exec.capture_stderr, Some(1234));
    /// ```
    pub fn capture_stderr(&mut self, count: usize) -> &mut Self {
        self.stderr(); // make sure stderr is captured
        self.capture_stderr = Some(count);
        self
    }

    /// Bind a file inside the sandbox to the specified file. Calling again this method will
    /// overwrite the previous value.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, File};
    /// use std::path::PathBuf;
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = File::new("test file");
    /// let uuid = file.uuid;
    /// exec.input(file, "test/file.txt", false);
    /// assert_eq!(exec.inputs[&PathBuf::from("test/file.txt")].file, uuid);
    /// ```
    pub fn input<F: Into<FileUuid>, P: Into<PathBuf>>(
        &mut self,
        file: F,
        path: P,
        executable: bool,
    ) -> &mut Self {
        self.inputs.insert(
            path.into(),
            ExecutionInput {
                file: file.into(),
                executable,
            },
        );
        self
    }

    /// Handle to a file produced by the execution. This should be called at least once before the
    /// evaluation starts in order to track the file. Calling this method more than once will
    /// return the same value.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    /// use std::path::PathBuf;
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// let file = exec.output("foo/bar.txt");
    /// assert_eq!(exec.outputs[&PathBuf::from("foo/bar.txt")], file);
    /// ```
    pub fn output<P: Into<PathBuf> + std::fmt::Debug>(&mut self, path: P) -> File {
        let file = File::new(format!("Output of '{}' at {:?}", self.description, path));
        self.outputs.entry(path.into()).or_insert(file).clone()
    }

    /// Add an environment variable to the execution.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("random exec", ExecutionCommand::local("foo"));
    /// exec.env("foo", "bar");
    /// assert_eq!(exec.env["foo"], "bar");
    /// ```
    pub fn env<S1: Into<String>, S2: Into<String>>(&mut self, key: S1, value: S2) -> &mut Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add an environment variable to the execution copying the value from the sandbox host.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand};
    ///
    /// let mut exec = Execution::new("random exec", ExecutionCommand::local("foo"));
    /// exec.copy_env("PATH");
    /// assert_eq!(&exec.copy_env[0], "PATH");
    /// ```
    pub fn copy_env<S: Into<String>>(&mut self, key: S) -> &mut Self {
        self.copy_env.push(key.into());
        self
    }

    /// Get a mutable reference to the execution limits.
    ///
    /// ```
    /// use task_maker_dag::{Execution, ExecutionCommand, ExecutionLimits};
    ///
    /// let mut exec = Execution::new("generator of prime numbers", ExecutionCommand::local("foo"));
    /// exec.limits_mut().cpu_time(2.0).sys_time(0.5).wall_time(3.0).memory(1024).allow_multiprocess();
    /// assert_eq!(exec.limits.cpu_time, Some(2.0));
    /// assert_eq!(exec.limits.sys_time, Some(0.5));
    /// assert_eq!(exec.limits.wall_time, Some(3.0));
    /// assert_eq!(exec.limits.memory, Some(1024));
    /// assert!(exec.limits.allow_multiprocess);
    /// ```
    pub fn limits_mut(&mut self) -> &mut ExecutionLimits {
        &mut self.limits
    }

    /// A reference to the configuration of the underlying DAG.
    pub fn config(&self) -> &ExecutionDAGConfig {
        &self.config
    }

    /// Set the tag of this `Execution`.
    pub fn tag(&mut self, tag: ExecutionTag) -> &mut Self {
        self.tag = Some(tag);
        self
    }

    /// Set the priority of this `Execution`.
    pub fn priority(&mut self, priority: Priority) -> &mut Self {
        self.priority = priority;
        self
    }

    /// Compute the [`ExecutionStatus`](struct.ExecutionStatus.html) based on the result of the
    /// execution, checking the signals, the return code and the time/memory constraints.
    pub fn status(
        &self,
        exit_status: u32,
        signal: Option<(u32, String)>,
        resources: &ExecutionResourcesUsage,
    ) -> ExecutionStatus {
        // it's important to check those before the signals because exceeding those
        // limits may trigger a SIGKILL from the sandbox
        if let Some(cpu_time_limit) = self.limits.cpu_time {
            if resources.cpu_time > cpu_time_limit {
                return ExecutionStatus::TimeLimitExceeded;
            }
        }
        if let Some(sys_time_limit) = self.limits.sys_time {
            if resources.sys_time > sys_time_limit {
                return ExecutionStatus::SysTimeLimitExceeded;
            }
        }
        if let Some(wall_time_limit) = self.limits.wall_time {
            if resources.wall_time > wall_time_limit {
                return ExecutionStatus::WallTimeLimitExceeded;
            }
        }
        if let Some(memory_limit) = self.limits.memory {
            if resources.memory > memory_limit {
                return ExecutionStatus::MemoryLimitExceeded;
            }
        }
        if let Some((signal, name)) = signal {
            return ExecutionStatus::Signal(signal, name);
        }
        if exit_status != 0 {
            return ExecutionStatus::ReturnCode(exit_status);
        }
        ExecutionStatus::Success
    }
}

impl std::fmt::Debug for ExecutionCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter
            .debug_struct("ExecutionCallbacks")
            .field("on_start", &self.on_start.len())
            .field("on_done", &self.on_done.len())
            .field("on_skip", &self.on_skip.len())
            .finish()?;
        Ok(())
    }
}

impl std::fmt::Debug for ExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionResult")
            .field("status", &self.status)
            .field("was_killed", &self.was_killed)
            .field("was_cached", &self.was_cached)
            .field("resources", &self.resources)
            .field(
                "stdout",
                &self
                    .stdout
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).to_string()),
            )
            .field(
                "stderr",
                &self
                    .stderr
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).to_string()),
            )
            .finish()
    }
}

impl From<&str> for ExecutionTag {
    fn from(name: &str) -> Self {
        ExecutionTag { name: name.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_success() {
        let exec = Execution::new("foo", ExecutionCommand::local("foo"));
        let status = exec.status(
            0,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 0.0,
                wall_time: 0.0,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::Success, status);
    }

    #[test]
    fn test_status_cpu_time() {
        let mut exec = Execution::new("foo", ExecutionCommand::local("foo"));
        exec.limits_mut().cpu_time(1.0);
        let status = exec.status(
            0,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 1.1,
                sys_time: 0.0,
                wall_time: 0.0,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::TimeLimitExceeded, status);
    }

    #[test]
    fn test_status_sys_time() {
        let mut exec = Execution::new("foo", ExecutionCommand::local("foo"));
        exec.limits_mut().sys_time(1.0);
        let status = exec.status(
            0,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 1.1,
                wall_time: 0.0,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::SysTimeLimitExceeded, status);
    }

    #[test]
    fn test_status_wall_time() {
        let mut exec = Execution::new("foo", ExecutionCommand::local("foo"));
        exec.limits_mut().wall_time(1.0);
        let status = exec.status(
            0,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 0.0,
                wall_time: 1.1,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::WallTimeLimitExceeded, status);
    }

    #[test]
    fn test_status_memory() {
        let mut exec = Execution::new("foo", ExecutionCommand::local("foo"));
        exec.limits_mut().memory(1234);
        let status = exec.status(
            0,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 0.0,
                wall_time: 0.0,
                memory: 1235,
            },
        );
        assert_eq!(ExecutionStatus::MemoryLimitExceeded, status);
    }

    #[test]
    fn test_status_signal() {
        let exec = Execution::new("foo", ExecutionCommand::local("foo"));
        let status = exec.status(
            0,
            Some((11, "Killed".into())),
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 0.0,
                wall_time: 0.0,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::Signal(11, "Killed".into()), status);
    }

    #[test]
    fn test_status_return_code() {
        let exec = Execution::new("foo", ExecutionCommand::local("foo"));
        let status = exec.status(
            1,
            None,
            &ExecutionResourcesUsage {
                cpu_time: 0.0,
                sys_time: 0.0,
                wall_time: 0.0,
                memory: 0,
            },
        );
        assert_eq!(ExecutionStatus::ReturnCode(1), status);
    }
}
