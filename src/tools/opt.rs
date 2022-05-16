use std::path::PathBuf;

use clap::Parser;

use crate::{ExecutionOpt, FilterOpt, FindTaskOpt, LoggerOpt, StorageOpt, UIOpt};

#[derive(Parser, Debug)]
#[clap(name = "task-maker-tools")]
pub struct Opt {
    #[clap(flatten)]
    pub logger: LoggerOpt,

    /// Which tool to use
    #[clap(subcommand)]
    pub tool: Tool,
}

#[derive(Parser, Debug)]
pub enum Tool {
    /// Clear a task directory
    Clear(ClearOpt),
    /// Generate the autocompletion files for the shell
    GenAutocompletion(GenAutocompletionOpt),
    /// Spawn an instance of the server
    Server(ServerOpt),
    /// Spawn an instance of a worker
    Worker(WorkerOpt),
    /// Print the TypeScript type definitions
    Typescriptify,
    /// Wipe the internal storage of task-maker
    ///
    /// Warning: no other instances of task-maker should be running when this flag is provided.
    Reset(ResetOpt),
    /// Run a command inside a sandbox similar to the one used by task-maker
    Sandbox(SandboxOpt),
    /// Obtain the information about a task.
    TaskInfo(TaskInfoOpt),
    /// Compile just the booklet for a task or a contest.
    Booklet(BookletOpt),
    /// Fuzz the checker of a task.
    FuzzChecker(FuzzCheckerOpt),
    /// Add the @check comments to the solutions.
    AddSolutionChecks(AddSolutionChecksOpt),
    /// Run the sandbox instead of the normal task-maker.
    ///
    /// This option is left as undocumented as it's not part of the public API.
    #[clap(hide = true)]
    InternalSandbox,
}

#[derive(Parser, Debug)]
pub struct ClearOpt {
    #[clap(flatten)]
    pub find_task: FindTaskOpt,
}

#[derive(Parser, Debug)]
pub struct GenAutocompletionOpt {
    /// Where to write the autocompletion files
    #[clap(short = 't', long = "target")]
    pub target: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
pub struct ServerOpt {
    /// Address to bind the server on for listening for the clients
    #[clap(default_value = "0.0.0.0:27182")]
    pub client_addr: String,

    /// Address to bind the server on for listening for the workers
    #[clap(default_value = "0.0.0.0:27183")]
    pub worker_addr: String,

    /// Password for the connection of the clients
    #[clap(long = "client-password")]
    pub client_password: Option<String>,

    /// Password for the connection of the workers
    #[clap(long = "worker-password")]
    pub worker_password: Option<String>,

    #[clap(flatten)]
    pub storage: StorageOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct WorkerOpt {
    /// Address to use to connect to a remote server
    pub server_addr: String,

    /// ID of the worker (to differentiate between multiple workers on the same machine).
    pub worker_id: Option<u32>,

    /// The name to use for the worker in remote executions
    #[clap(long)]
    pub name: Option<String>,

    #[clap(flatten)]
    pub storage: StorageOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct ResetOpt {
    #[clap(flatten)]
    pub storage: StorageOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct SandboxOpt {
    /// Working directory of the sandbox.
    ///
    /// Will be mounted in /box inside the sandbox. Defaults to current working directory.
    #[clap(long, short)]
    pub workdir: Option<PathBuf>,

    /// Memory limit for the sandbox, in KiB.
    #[clap(long, short)]
    pub memory_limit: Option<u64>,

    /// Stack limit for the sandbox, in KiB.
    #[clap(long, short)]
    pub stack_limit: Option<u64>,

    /// Prevent forking.
    #[clap(long)]
    pub single_process: bool,

    /// List of additional directory mounted read-only inside the sandbox.
    #[clap(long, short)]
    pub readable_dirs: Vec<PathBuf>,

    /// Mount /tmp and /dev/null inside the sandbox
    #[clap(long)]
    pub mount_tmpfs: bool,

    /// User id.
    #[clap(long, default_value = "1000")]
    pub uid: usize,

    /// User id.
    #[clap(long, default_value = "1000")]
    pub gid: usize,

    /// Command to execute inside the sandbox. If not specified, bash is executed.
    pub command: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct TaskInfoOpt {
    #[clap(flatten)]
    pub find_task: FindTaskOpt,
    /// Produce JSON output.
    #[clap(long, short)]
    pub json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct BookletOpt {
    /// Include the solutions in the booklet
    #[clap(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Directory of the context.
    ///
    /// When specified, --task-dir should not be used.
    #[clap(short = 'c', long = "contest-dir")]
    pub contest_dir: Option<PathBuf>,

    /// Directory of the task.
    ///
    /// When specified, --contest-dir should not be used.
    #[clap(short = 't', long = "task-dir")]
    pub task_dir: Vec<PathBuf>,

    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,

    #[clap(flatten)]
    pub ui: UIOpt,

    #[clap(flatten)]
    pub execution: ExecutionOpt,

    #[clap(flatten)]
    pub storage: StorageOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct FuzzCheckerOpt {
    #[clap(flatten)]
    pub find_task: FindTaskOpt,

    /// Where to store fuzzing data.
    ///
    /// The path is relative to the task directory.
    #[clap(long, default_value = "fuzz")]
    pub fuzz_dir: PathBuf,

    /// Additional sanitizers to use.
    ///
    /// Comma separated list of sanitizers to use.
    #[clap(long, default_value = "address,undefined")]
    pub sanitizers: String,

    /// List of additional arguments to pass to the compiler.
    ///
    /// If nothing is listed here, -O2 and -g are passed.
    pub extra_args: Vec<String>,

    /// Number of fuzzing process to spawn.
    ///
    /// Defaults to the number of cores.
    #[clap(long, short)]
    pub jobs: Option<usize>,

    /// Maximum number of seconds the checker can run.
    ///
    /// If the checker takes longer than this, the fuzzer fails and the corresponding file is
    /// emitted.
    #[clap(long, default_value = "2")]
    pub checker_timeout: usize,

    /// Maximum fuzzing time in seconds.
    ///
    /// Halt after fuzzing for this amount of time. Zero should not be used.
    #[clap(long, default_value = "60")]
    pub max_time: usize,

    /// Don't print the fuzzer output to the console, but redirect it to a file.
    #[clap(long)]
    pub quiet: bool,

    /// Don't run the evaluation for building the output files.
    #[clap(long)]
    pub no_build: bool,

    #[clap(flatten)]
    pub execution: ExecutionOpt,

    #[clap(flatten)]
    pub storage: StorageOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct AddSolutionChecksOpt {
    #[clap(flatten)]
    pub find_task: FindTaskOpt,

    #[clap(flatten)]
    pub ui: UIOpt,

    #[clap(flatten)]
    pub storage: StorageOpt,

    #[clap(flatten)]
    pub filter: FilterOpt,

    #[clap(flatten)]
    pub execution: ExecutionOpt,

    /// Write the @check directly to the solution files.
    ///
    /// Warning: while this is generally safe, make sure to have a way of reverting the changes.
    #[clap(long, short)]
    pub in_place: bool,
}
