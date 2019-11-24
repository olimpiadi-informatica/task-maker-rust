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

    /// Evaluate only the solution with the specified path
    ///
    /// The solution can reside anywhere in the filesystem.
    #[structopt(long = "solution", short = "-s")]
    pub solution: Vec<PathBuf>,

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

    /// Include the solutions in the booklet.
    #[structopt(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Do not build the statement files and the booklets.
    #[structopt(long = "no-statement")]
    pub no_statement: bool,

    /// Verbose mode (-v, -vv, -vvv, etc.). Note that it does not play well with curses ui.
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,

    /// Run a server instead of the normal task-maker.
    #[structopt(long = "server")]
    pub server: bool,

    /// Address to bind the server on.
    ///
    /// This option only has effect with `--server`.
    #[structopt(long = "server-address-clients", default_value = "0.0.0.0:27182")]
    pub server_address_clients: String,

    /// Address to bind the server on.
    ///
    /// This option only has effect with `--server`.
    #[structopt(long = "server-address-workers", default_value = "0.0.0.0:27183")]
    pub server_address_workers: String,
}

impl Opt {
    /// Make an `EvaluationConfig` from this command line options.
    pub fn to_config(&self) -> EvaluationConfig {
        EvaluationConfig {
            solution_filter: self.filter.clone(),
            booklet_solutions: self.booklet_solutions,
            no_statement: self.no_statement,
            solution_paths: self.solution.clone(),
        }
    }
}
