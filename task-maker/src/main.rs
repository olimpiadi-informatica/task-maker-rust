#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

extern crate task_maker_dag;
extern crate task_maker_exec;
extern crate task_maker_lang;
extern crate task_maker_store;

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate uuid;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate boxfnonce;
extern crate glob;
extern crate itertools;
#[cfg(test)]
extern crate pretty_assertions;
extern crate structopt;
extern crate tempdir;
extern crate termcolor;
extern crate termion;
extern crate tui;

pub mod evaluation;
pub mod score_types;
pub mod task_types;
pub mod ui;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "task-maker")]
struct Opt {
    /// Directory of the task to evaluate
    #[structopt(short = "t", long = "task-dir", default_value = ".")]
    task_dir: PathBuf,

    /// Which UI to use, available UIS are: print, raw, curses
    #[structopt(long = "ui", default_value = "print")]
    ui: crate::ui::UIType,
}

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let opt = Opt::from_args();

    use crate::evaluation::*;
    use crate::task_types::ioi::*;
    use std::sync::{Arc, Mutex};
    use task_maker_exec::executors::LocalExecutor;
    use task_maker_store::*;

    let (eval, receiver) = EvaluationData::new();
    let ui = opt.ui;
    let ui_thread = std::thread::Builder::new()
        .name("UI".to_owned())
        .spawn(move || {
            ui.start(receiver);
        })
        .unwrap();

    use crate::task_types::TaskFormat;
    if !task_types::ioi::formats::IOIItalianYaml::is_valid(&opt.task_dir) {
        panic!("Invalid task directory!");
    }
    let task = task_types::ioi::formats::IOIItalianYaml::parse(&opt.task_dir).unwrap();

    let cwd = tempdir::TempDir::new("task-maker").unwrap();
    let store_path = cwd.path().join("store");
    let file_store = FileStore::new(&store_path).expect("Cannot create the file store");
    let executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
    task.evaluate(eval, &IOIEvaluationOptions {}, executor);
    ui_thread.join().unwrap();
}
