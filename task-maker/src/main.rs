#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

#[macro_use]
extern crate log;

use std::path::PathBuf;
use std::sync::{mpsc::channel, Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use task_maker_cache::Cache;
use task_maker_exec::{executors::LocalExecutor, ExecutorClient};
use task_maker_format::{ioi, EvaluationData, TaskFormat};
use task_maker_store::*;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "task-maker",
    raw(setting = "structopt::clap::AppSettings::ColoredHelp")
)]
struct Opt {
    /// Directory of the task to evaluate
    #[structopt(short = "t", long = "task-dir", default_value = ".")]
    task_dir: PathBuf,

    /// Which UI to use, available UIS are: print, raw, curses
    #[structopt(long = "ui", default_value = "print")]
    ui: task_maker_format::ui::UIType,

    /// Keep all the sandbox directories
    #[structopt(long = "keep-sandboxes")]
    keep_sandboxes: bool,

    /// Do not write any file inside the task directory
    #[structopt(long = "dry-run")]
    dry_run: bool,

    /// The level of caching to use
    #[structopt(long = "cache")]
    cache_mode: Option<String>,

    /// Do not run in parallel time critical executions on the same machine
    #[structopt(long = "exclusive")]
    exclusive: bool,

    /// Give to the solution some extra time before being killed
    #[structopt(long = "extra-time")]
    extra_time: Option<f64>,

    /// Copy the executables to the bin/ folder
    #[structopt(long = "copy-exe")]
    copy_exe: bool,

    /// Execute only the solutions whose names start with the filter
    #[structopt(long = "filter")]
    filter: Vec<String>,

    /// Look at most for this number of parents for searching the task
    #[structopt(long = "max-depth", default_value = "3")]
    max_depth: u32,

    /// Clear the task directory and exit
    #[structopt(long = "clean")]
    clean: bool,
}

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let opt = Opt::from_args();

    if opt.cache_mode.is_some()
        || opt.exclusive
        || opt.extra_time.is_some()
        || opt.copy_exe
        || !opt.filter.is_empty()
        || opt.max_depth != 3
        || opt.clean
    {
        unimplemented!("This option is not implemented yet");
    }

    let (mut eval, receiver) = EvaluationData::new();
    eval.dag
        .config_mut()
        .keep_sandboxes(opt.keep_sandboxes)
        .dry_run(opt.dry_run);

    // setup the task
    let task: Box<dyn TaskFormat> = if let Ok(task) = ioi::Task::new(&opt.task_dir) {
        debug!("The task is IOI: {:#?}", task);
        Box::new(task)
    } else {
        panic!("Invalid task directory!");
    };

    // setup the ui thread
    let mut ui = task.ui(opt.ui).unwrap();
    let ui_thread = std::thread::Builder::new()
        .name("UI".to_owned())
        .spawn(move || {
            while let Ok(message) = receiver.recv() {
                ui.on_message(message);
            }
            ui.finish();
        })
        .unwrap();

    // setup the executor
    let cwd = tempdir::TempDir::new("task-maker").unwrap();
    let store_path = cwd.path().join("store");
    let file_store = FileStore::new(&store_path).expect("Cannot create the file store");
    let cache = Cache::new(&store_path).expect("Cannot create the cache");
    let mut executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), cache, 4);

    // build the DAG for the task
    task.execute(&mut eval).unwrap();

    trace!("The DAG is: {:#?}", eval.dag);

    // start the server and the client
    let (tx, rx_remote) = channel();
    let (tx_remote, rx) = channel();
    let server = thread::Builder::new()
        .name("Executor thread".into())
        .spawn(move || {
            executor.evaluate(tx_remote, rx_remote).unwrap();
        })
        .unwrap();
    ExecutorClient::evaluate(eval.dag, tx, &rx).unwrap();

    // wait for the server and the ui to exit
    server.join().expect("Executor panicked");
    drop(eval.sender); // make the UI exit
    ui_thread.join().expect("UI panicked");
}
