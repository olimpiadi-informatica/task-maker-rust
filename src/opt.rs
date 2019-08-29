use std::path::PathBuf;
use structopt::StructOpt;
use task_maker_format::EvaluationConfig;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "task-maker",
    raw(setting = "structopt::clap::AppSettings::ColoredHelp")
)]
pub struct Opt {
    /// Directory of the task to evaluate
    #[structopt(short = "t", long = "task-dir", default_value = ".")]
    pub task_dir: PathBuf,

    /// Which UI to use, available UIS are: print, raw, curses, json.
    ///
    /// Note that the JSON api is not stable yet.
    #[structopt(long = "ui", default_value = "print")]
    pub ui: task_maker_format::ui::UIType,

    /// Keep all the sandbox directories
    #[structopt(long = "keep-sandboxes")]
    pub keep_sandboxes: bool,

    /// Do not write any file inside the task directory
    #[structopt(long = "dry-run")]
    pub dry_run: bool,

    /// Disable the cache for this comma separated list of tags
    ///
    /// Providing an empty list will disable all the caches. The supported tags are: compilation,
    /// generation, evaluation, checking, booklet.
    #[structopt(long = "no-cache")]
    #[allow(clippy::option_option)]
    pub no_cache: Option<Option<String>>,

    /// Do not run in parallel time critical executions on the same machine
    #[structopt(long = "exclusive")]
    pub exclusive: bool,

    /// Give to the solution some extra time before being killed
    #[structopt(long = "extra-time")]
    pub extra_time: Option<f64>,

    /// Copy the executables to the bin/ folder
    #[structopt(long = "copy-exe")]
    pub copy_exe: bool,

    /// Execute only the solutions whose names start with the filter
    ///
    /// Note that just the file name is checked (e.g. sol.cpp is the same as sol/sol.cpp). Without
    /// specifying anything all the solutions are executed.
    pub filter: Vec<String>,

    /// Look at most for this number of parents for searching the task
    #[structopt(long = "max-depth", default_value = "3")]
    pub max_depth: u32,

    /// Clear the task directory and exit
    #[structopt(long = "clean")]
    pub clean: bool,

    /// Where to store the storage files, including the cache
    #[structopt(long = "store-dir")]
    pub store_dir: Option<PathBuf>,

    /// The number of CPU cores to use
    #[structopt(long = "num-cores")]
    pub num_cores: Option<usize>,
}

impl Opt {
    /// Make an `EvaluationConfig` from this command line options.
    pub fn to_config(&self) -> EvaluationConfig {
        EvaluationConfig {
            solution_filter: self.filter.clone(),
        }
    }
}
