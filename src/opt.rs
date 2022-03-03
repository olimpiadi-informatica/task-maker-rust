use std::path::PathBuf;

use anyhow::{Context, Error};
use itertools::Itertools;
use structopt::StructOpt;

use task_maker_dag::DagPriority;
use task_maker_format::terry::Seed;
use task_maker_format::{find_task, get_sanity_check_names, TaskFormat};
use task_maker_format::{EvaluationConfig, VALID_TAGS};

#[derive(StructOpt, Debug)]
#[structopt(
    name = "task-maker",
    version = include_str!(concat!(env!("OUT_DIR"), "/version.txt")),
    setting = structopt::clap::AppSettings::ColoredHelp,
)]
pub struct Opt {
    #[structopt(flatten)]
    pub find_task: FindTaskOpt,

    #[structopt(flatten)]
    pub ui: UIOpt,

    /// Do not run in parallel time critical executions on the same machine
    #[structopt(long = "exclusive")]
    pub exclusive: bool,

    /// Execute only the solutions whose names start with the filter
    ///
    /// Note that just the file name is checked (e.g. sol.cpp is the same as sol/sol.cpp). Without
    /// specifying anything all the solutions are executed.
    pub filter: Vec<String>,

    /// Evaluate only the solution with the specified path
    ///
    /// The solution can reside anywhere in the filesystem.
    #[structopt(long = "solution", short = "-s")]
    pub solution: Vec<PathBuf>,

    /// Force this seed instead of a random one in Terry.
    #[structopt(long)]
    pub seed: Option<Seed>,

    /// Clear the task directory and exit
    ///
    /// Deprecated: Use `task-maker-tools clear`
    #[structopt(long = "clean")]
    pub clean: bool,

    /// Include the solutions in the booklet
    #[structopt(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Do not build the statement files and the booklets
    #[structopt(long = "no-statement")]
    pub no_statement: bool,

    /// List of sanity checks to skip.
    #[structopt(short = "-W", long_help = skip_sanity_checks_long_help())]
    pub skip_sanity_checks: Vec<String>,

    #[structopt(flatten)]
    pub logger: LoggerOpt,

    #[structopt(flatten)]
    pub storage: StorageOpt,

    #[structopt(flatten)]
    pub execution: ExecutionOpt,
}

#[derive(StructOpt, Debug, Clone)]
pub struct LoggerOpt {
    /// Verbose mode (-v, -vv, -vvv, etc.). Note that it does not play well with curses ui.
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(StructOpt, Debug, Clone)]
pub struct FindTaskOpt {
    /// Directory of the task
    #[structopt(short = "t", long = "task-dir", default_value = "")]
    pub task_dir: PathBuf,

    /// Look at most for this number of parents for searching the task
    #[structopt(long = "max-depth", default_value = "3")]
    pub max_depth: u32,
}

#[derive(StructOpt, Debug, Clone)]
pub struct UIOpt {
    /// Which UI to use, available UIs are: print, raw, curses, json.
    ///
    /// Note that the JSON api is not stable yet.
    #[structopt(long = "ui", default_value = "curses")]
    pub ui: task_maker_format::ui::UIType,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ExecutionOpt {
    /// Keep all the sandbox directories
    #[structopt(long = "keep-sandboxes")]
    pub keep_sandboxes: bool,

    /// Do not write any file inside the task directory
    #[structopt(long = "dry-run")]
    pub dry_run: bool,

    /// Disable the cache for this comma separated list of tags
    #[structopt(long = "no-cache", long_help = no_cache_long_help())]
    #[allow(clippy::option_option)]
    pub no_cache: Option<Option<String>>,

    /// Give to the solution some extra time before being killed
    #[structopt(long = "extra-time")]
    pub extra_time: Option<f64>,

    /// Copy the executables to the bin/ folder
    #[structopt(long = "copy-exe")]
    pub copy_exe: bool,

    /// Copy the logs of some executions to the bin/logs/ folder
    #[structopt(long = "copy-logs")]
    pub copy_logs: bool,

    /// Store the DAG in DOT format inside of bin/DAG.dot
    #[structopt(long = "copy-dag")]
    pub copy_dag: bool,

    /// The number of CPU cores to use.
    #[structopt(long = "num-cores")]
    pub num_cores: Option<usize>,

    /// Run the evaluation on a remote server instead of locally
    #[structopt(long = "evaluate-on")]
    pub evaluate_on: Option<String>,

    /// The name to use for the client in remote executions
    #[structopt(long)]
    pub name: Option<String>,

    /// Priority of the evaluations spawned by this invocation of task-maker; no effect if running
    /// locally.
    #[structopt(long, default_value = "0")]
    pub priority: DagPriority,
}

#[derive(StructOpt, Debug, Clone)]
pub struct StorageOpt {
    /// Where to store the storage files, including the cache
    #[structopt(long = "store-dir")]
    pub store_dir: Option<PathBuf>,

    /// Maximum size of the storage directory, in MiB
    #[structopt(long = "max-cache", default_value = "3072")]
    pub max_cache: u64,

    /// When the storage is flushed, this is the new maximum size, in MiB.
    #[structopt(long = "min-cache", default_value = "2048")]
    pub min_cache: u64,
}

/// Returns the long-help for the "skip sanity checks" option.
fn skip_sanity_checks_long_help() -> &'static str {
    lazy_static! {
        pub static ref DOC: String = format!(
            "List of sanity checks to skip.\n\nThe available checks are: {}.",
            get_sanity_check_names()
        );
    }
    &DOC
}

/// Returns the long-help for the --no-cache option.
fn no_cache_long_help() -> &'static str {
    lazy_static! {
        pub static ref DOC: String = format!(
            "Disable the cache for this comma separated list of tags\n\nProviding an empty list will disable all the caches. The supported tags are: {}.",
            VALID_TAGS.iter().join(", ")
        );
    }
    &DOC
}

impl Opt {
    /// Make an `EvaluationConfig` from this command line options.
    pub fn to_config(&self) -> EvaluationConfig {
        EvaluationConfig {
            solution_filter: self.filter.clone(),
            booklet_solutions: self.booklet_solutions,
            no_statement: self.no_statement,
            solution_paths: self.solution.clone(),
            disabled_sanity_checks: self.skip_sanity_checks.clone(),
            seed: self.seed,
            dry_run: self.execution.dry_run,
        }
    }

    pub fn enable_log(&mut self) {
        self.logger.enable_log();
        self.ui.disable_if_needed(&self.logger);
    }
}

impl UIOpt {
    /// Disable the Curses UI and fallback to PrintUI if verbose output is enabled.
    pub fn disable_if_needed(&mut self, logger: &LoggerOpt) {
        let mut show_warning = false;
        if logger.should_diable_curses() {
            if let task_maker_format::ui::UIType::Curses = self.ui {
                // warning deferred to after the logger has been initialized
                show_warning = true;
                self.ui = task_maker_format::ui::UIType::Print;
            }
        }
        if show_warning {
            warn!("Do not combine -v with curses ui, bad things will happen! Fallback to print ui");
        }
    }
}

impl StorageOpt {
    /// Get the store directory of this configuration. If nothing is specified a cache directory is
    /// used if available, otherwise a temporary directory.
    pub fn store_dir(&self) -> PathBuf {
        match &self.store_dir {
            Some(dir) => dir.clone(),
            None => {
                let project = directories::ProjectDirs::from("", "", "task-maker");
                if let Some(project) = project {
                    project.cache_dir().to_owned()
                } else {
                    std::env::temp_dir().join("task-maker")
                }
            }
        }
    }
}

impl LoggerOpt {
    /// Enable the logs according to the specified configuration.
    pub fn enable_log(&self) {
        if self.verbose > 0 {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
        match self.verbose {
            0 => std::env::set_var("RUST_LOG", "warn,tabox=warn"),
            1 => std::env::set_var("RUST_LOG", "info,tabox=info"),
            2 => std::env::set_var("RUST_LOG", "debug,tabox=debug"),
            _ => std::env::set_var("RUST_LOG", "trace,tabox=trace"),
        }

        env_logger::Builder::from_default_env()
            .format_timestamp_nanos()
            .init();
        better_panic::install();
    }

    pub fn should_diable_curses(&self) -> bool {
        self.verbose > 0
    }
}

impl FindTaskOpt {
    /// Use the specified options to find a task.
    pub fn find_task(&self, eval_config: &EvaluationConfig) -> Result<TaskFormat, Error> {
        find_task(&self.task_dir, self.max_depth, eval_config).context("Invalid task directory")
    }
}
