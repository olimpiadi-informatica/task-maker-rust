//! The new cmsMake!
//!
//! # Installation
//! The official way for installing `task-maker-rust` is not defined yet.
//!
//! For now you should clone the repo (with `--recurse-submodules`!) and run `cargo build --release`.
//! The executable should be located at `target/release/task-maker`.
//!
//! # Usage
//!
//! ## Simple local usage
//! Run `task-maker` in the task folder to compile and run everything.
//!
//! Specifying no option all the caches are active, the next executions will be very fast, actually doing only what's needed.
//!
//! ## Disable cache
//! If you really want to repeat the execution of something provide the `--no-cache`
//! option:
//! ```bash
//! task-maker --no-cache
//! ```
//!
//! Without any options `--no-cache` won't use any caches.
//!
//! If you want, for example, just redo the evaluations (maybe for retrying the timings), use `--no-cache=evaluation`.
//! The available options for `--no-cache` can be found with `--help`.
//!
//! ## Test only a subset of solutions
//! Sometimes you only want to test only some solutions, speeding up the compilation and cleaning a bit the output:
//! ```bash
//! task-maker sol1.cpp sol2.py
//! ```
//! Note that you may or may not specify the folder of the solution (sol/ or solution/).
//! You can also specify only the prefix of the name of the solutions you want to check.
//!
//! ## Using different task directory
//! By default the task in the current directory is executed, if you want to change the task without `cd`-ing away:
//! ```bash
//! task-maker --task-dir ~/tasks/poldo
//! ```
//!
//! ## Extracting executable files
//! All the compiled files are kept in an internal folder but if you want to use them, for example to debug a solution, passing `--copy-exe` all the useful files are copied to the `bin/` folder inside the task directory.
//! ```bash
//! task-maker --copy-exe
//! ```
//!
//! ## Clean the task directory
//! If you want to clean everything, for example after the contest, simply run:
//! ```bash
//! task-maker --clean
//! ```
//! This will remove the files that can be regenerated from the task directory.
//! Note that the internal cache is not pruned by this command.

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
use task_maker_format::ui::UIMessage;
use task_maker_format::{ioi, EvaluationData, TaskFormat, UISender};
use task_maker_store::*;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let opt = opt::Opt::from_args();

    if opt.exclusive {
        unimplemented!("This option is not implemented yet");
    }

    // setup the task
    let task: Box<dyn TaskFormat> =
        find_task(&opt.task_dir, opt.max_depth).expect("Invalid task directory!");

    // clean the task
    if opt.clean {
        task.clean().expect("Cannot clean task directory!");
        return;
    }

    // setup the configuration and the evaluation metadata
    let (mut eval, receiver) = EvaluationData::new();
    let eval_config = opt.to_config();
    let config = eval.dag.config_mut();
    config
        .keep_sandboxes(opt.keep_sandboxes)
        .dry_run(opt.dry_run)
        .cache_mode(CacheMode::from(opt.no_cache))
        .copy_exe(opt.copy_exe);
    if let Some(extra_time) = opt.extra_time {
        assert!(extra_time >= 0.0, "the extra time cannot be negative");
        config.extra_time(extra_time);
    }

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
            (cwd.path().to_owned(), Some(cwd))
        }
    };
    let file_store =
        FileStore::new(store_path.join("store")).expect("Cannot create the file store");
    let cache = Cache::new(store_path.join("cache")).expect("Cannot create the cache");
    let num_cores = opt.num_cores.unwrap_or_else(|| num_cpus::get());
    let sandbox_path = store_path.join("sandboxes");
    let mut executor = LocalExecutor::new(
        Arc::new(Mutex::new(file_store)),
        cache,
        num_cores,
        sandbox_path,
    );

    // build the DAG for the task
    task.execute(&mut eval, &eval_config).unwrap();

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

    let ui_sender = eval.sender.clone();
    ExecutorClient::evaluate(eval.dag, tx, &rx, move |status| {
        ui_sender.send(UIMessage::ServerStatus { status }).unwrap();
    })
    .unwrap();

    // wait for the server and the ui to exit
    server.join().expect("Executor panicked");
    drop(eval.sender); // make the UI exit
    ui_thread.join().expect("UI panicked");
}

/// Search for a valid task directory, starting from base and going _at most_ `max_depth` times up.
fn find_task<P: Into<PathBuf>>(base: P, max_depth: u32) -> Option<Box<dyn TaskFormat>> {
    let mut base = base.into();
    for _ in 0..=max_depth {
        if base.join("task.yaml").exists() {
            break;
        }
        base = match base.parent() {
            Some(parent) => parent.into(),
            _ => break,
        };
    }
    match ioi::Task::new(&base) {
        Ok(task) => {
            trace!("The task is IOI: {:#?}", task);
            return Some(Box::new(task));
        }
        Err(e) => {
            error!("Invalid task: {:?}", e);
            None
        }
    }
}
