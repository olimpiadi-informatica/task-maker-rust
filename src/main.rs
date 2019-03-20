#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cyclomatic_complexity)]
#![allow(clippy::too_many_arguments)]

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate uuid;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;
extern crate fs2;
extern crate hex;
extern crate pest;
#[macro_use]
extern crate pest_derive;
#[cfg(test)]
extern crate pretty_assertions;
extern crate tempdir;
extern crate which;
#[macro_use]
extern crate lazy_static;
extern crate boxfnonce;
extern crate glob;
extern crate itertools;
extern crate regex;
extern crate structopt;
extern crate termcolor;
extern crate termion;
extern crate tui;

pub mod evaluation;
pub mod execution;
pub mod executor;
pub mod languages;
pub mod score_types;
pub mod store;
pub mod task_types;
#[cfg(test)]
mod test_utils;
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
    use crate::executor::*;
    use crate::store::*;
    use crate::task_types::ioi::*;
    use std::sync::{Arc, Mutex};

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
