use std::path::PathBuf;

use anyhow::{Context, Error};
use clap::{ArgAction, Parser};
use itertools::Itertools;

use task_maker_dag::DagPriority;
use task_maker_format::terry::Seed;
use task_maker_format::{find_task, get_sanity_check_names, TaskFormat};
use task_maker_format::{EvaluationConfig, VALID_TAGS};

#[derive(Parser, Debug)]
#[clap(
    name = "task-maker",
    version = include_str!(concat!(env!("OUT_DIR"), "/version.txt")),
)]
pub struct Opt {
    #[clap(flatten, next_help_heading = Some("FILTER"))]
    pub filter: FilterOpt,

    #[clap(flatten, next_help_heading = Some("TASK SEARCH"))]
    pub find_task: FindTaskOpt,

    #[clap(flatten, next_help_heading = Some("UI"))]
    pub ui: UIOpt,

    /// Do not run in parallel time critical executions on the same machine
    #[clap(long = "exclusive")]
    pub exclusive: bool,

    #[clap(flatten, next_help_heading = Some("TERRY"))]
    pub terry: TerryOpt,

    /// Clear the task directory and exit
    ///
    /// Deprecated: Use `task-maker-tools clear`
    #[clap(long = "clean")]
    pub clean: bool,

    #[clap(flatten, next_help_heading = Some("BOOKLET"))]
    pub booklet: BookletOpt,

    /// List of sanity checks to skip.
    #[clap(short = 'W', long = "skip-checks", long_help = skip_sanity_checks_long_help())]
    pub skip_sanity_checks: Vec<String>,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: StorageOpt,

    #[clap(flatten, next_help_heading = Some("EXECUTION"))]
    pub execution: ExecutionOpt,

    #[clap(flatten, next_help_heading = Some("LOGGING"))]
    pub logger: LoggerOpt,
}

#[derive(Parser, Debug, Clone)]
pub struct LoggerOpt {
    /// Verbose mode (-v, -vv, -vvv, etc.). Note that it does not play well with curses ui.
    #[clap(short, long, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Parser, Debug, Clone)]
pub struct FindTaskOpt {
    /// Directory of the task
    #[clap(short = 't', long = "task-dir")]
    pub task_dir: Option<PathBuf>,

    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,
}

#[derive(Parser, Debug, Clone)]
pub struct UIOpt {
    /// Which UI to use, available UIs are: print, raw, curses, json.
    ///
    /// Note that the JSON api is not stable yet.
    #[clap(long = "ui", default_value = "curses")]
    pub ui: task_maker_format::ui::UIType,
}

#[derive(Parser, Debug, Clone)]
pub struct ExecutionOpt {
    /// Keep all the sandbox directories
    #[clap(long = "keep-sandboxes")]
    pub keep_sandboxes: bool,

    /// Do not write any file inside the task directory
    #[clap(long = "dry-run")]
    pub dry_run: bool,

    /// Disable the cache for this comma separated list of tags
    #[clap(long = "no-cache", long_help = no_cache_long_help(), require_equals = true)]
    #[allow(clippy::option_option)]
    pub no_cache: Option<Option<String>>,

    /// Give to the solution some extra time before being killed
    #[clap(long = "extra-time")]
    pub extra_time: Option<f64>,

    /// Give to the solution some extra memory before being killed
    #[clap(long = "extra-memory")]
    pub extra_memory: Option<u64>,

    /// Copy the executables to the bin/ folder
    #[clap(long = "copy-exe")]
    pub copy_exe: bool,

    /// Copy the logs of some executions to the bin/logs/ folder
    #[clap(long = "copy-logs")]
    pub copy_logs: bool,

    /// Store the DAG in DOT format inside of bin/DAG.dot
    #[clap(long = "copy-dag")]
    pub copy_dag: bool,

    /// The number of CPU cores to use.
    #[clap(long = "num-cores")]
    pub num_cores: Option<usize>,

    /// Run the evaluation on a remote server instead of locally
    #[clap(long = "evaluate-on")]
    pub evaluate_on: Option<String>,

    /// The name to use for the client in remote executions
    #[clap(long)]
    pub name: Option<String>,

    /// Priority of the evaluations spawned by this invocation of task-maker; no effect if running
    /// locally.
    #[clap(long, default_value = "0")]
    pub priority: DagPriority,
}

#[derive(Parser, Debug, Clone)]
pub struct StorageOpt {
    /// Where to store the storage files, including the cache
    #[clap(long = "store-dir")]
    pub store_dir: Option<PathBuf>,

    /// Maximum size of the storage directory, in MiB
    #[clap(long = "max-cache", default_value = "3072")]
    pub max_cache: u64,

    /// When the storage is flushed, this is the new maximum size, in MiB.
    #[clap(long = "min-cache", default_value = "2048")]
    pub min_cache: u64,
}

#[derive(Parser, Debug, Clone)]
pub struct FilterOpt {
    /// Execute only the solutions whose names start with the filter
    ///
    /// Note that just the file name is checked (e.g. sol.cpp is the same as sol/sol.cpp). Without
    /// specifying anything all the solutions are executed.
    pub filter: Vec<String>,

    /// Evaluate only the solution with the specified path
    ///
    /// The solution can reside anywhere in the filesystem.
    #[clap(long, short)]
    pub solution: Vec<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
pub struct TerryOpt {
    /// Force this seed instead of a random one.
    #[clap(long)]
    pub seed: Option<Seed>,
}

#[derive(Parser, Debug, Clone)]
pub struct BookletOpt {
    /// Include the solutions in the booklet
    #[clap(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Do not build the statement files and the booklets
    #[clap(long = "no-statement")]
    pub no_statement: bool,
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
            solution_filter: self.filter.filter.clone(),
            booklet_solutions: self.booklet.booklet_solutions,
            no_statement: self.booklet.no_statement,
            solution_paths: self.filter.solution.clone(),
            disabled_sanity_checks: self.skip_sanity_checks.clone(),
            seed: self.terry.seed,
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
        find_task(self.task_dir.clone(), self.max_depth, eval_config)
            .context("Invalid task directory")
    }
}
