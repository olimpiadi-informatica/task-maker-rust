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
use task_maker_exec::{executors::LocalExecutor, ExecutorClient};
use task_maker_format::{ioi, EvaluationData, TaskFormat};
use task_maker_store::*;

#[derive(StructOpt, Debug)]
#[structopt(name = "task-maker")]
struct Opt {
    /// Directory of the task to evaluate
    #[structopt(short = "t", long = "task-dir", default_value = ".")]
    task_dir: PathBuf,

    /// Which UI to use, available UIS are: print, raw, curses
    #[structopt(long = "ui", default_value = "print")]
    ui: task_maker_format::ui::UIType,
}

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let opt = Opt::from_args();

    // setup the task
    let task: Box<dyn TaskFormat> = if let Ok(task) = ioi::Task::new(&opt.task_dir) {
        debug!("The task is IOI: {:#?}", task);
        Box::new(task)
    } else {
        panic!("Invalid task directory!");
    };

    let (mut eval, receiver) = EvaluationData::new();

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
    let mut executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);

    // build the DAG for the task
    task.execute(&mut eval).unwrap();

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
