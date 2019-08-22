#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

#[macro_use]
extern crate log;

mod opt;

use std::path::PathBuf;
use std::sync::{mpsc::channel, Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use task_maker_cache::Cache;
use task_maker_dag::CacheMode;
use task_maker_exec::{executors::LocalExecutor, ExecutorClient};
use task_maker_format::{ioi, EvaluationData, TaskFormat};
use task_maker_store::*;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let opt = opt::Opt::from_args();

    if opt.exclusive
        || opt.extra_time.is_some()
        || opt.copy_exe
        || !opt.filter.is_empty()
        || opt.clean
    {
        unimplemented!("This option is not implemented yet");
    }

    let (mut eval, receiver) = EvaluationData::new();
    eval.dag
        .config_mut()
        .keep_sandboxes(opt.keep_sandboxes)
        .dry_run(opt.dry_run)
        .cache_mode(CacheMode::from(opt.no_cache));

    // setup the task
    let task: Box<dyn TaskFormat> =
        find_task(opt.task_dir, opt.max_depth).expect("Invalid task directory!");

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
    let (store_path, _tempdir) = match opt.store_dir {
        Some(dir) => (dir, None),
        None => {
            let cwd = tempdir::TempDir::new("task-maker").unwrap();
            (cwd.path().join("store"), Some(cwd))
        }
    };
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

/// Search for a valid task directory, starting from base and going _at most_ `max_depth` times up.
fn find_task<P: Into<PathBuf>>(base: P, max_depth: u32) -> Option<Box<dyn TaskFormat>> {
    let mut base = base.into();
    for _ in 0..=max_depth {
        if let Ok(task) = ioi::Task::new(&base) {
            debug!("The task is IOI: {:#?}", task);
            return Some(Box::new(task));
        }
        base = match base.parent() {
            Some(parent) => parent.into(),
            _ => break,
        };
    }
    None
}
