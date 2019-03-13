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
extern crate regex;

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

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    println!("Tmbox: {}/bin/tmbox", env!("OUT_DIR"));
    use crate::evaluation::*;
    use crate::executor::*;
    use crate::store::*;
    use crate::task_types::ioi::*;
    use crate::ui::*;
    use std::sync::{Arc, Mutex};

    let (eval, receiver) = EvaluationData::new();
    eval.sender
        .send(UIMessage::Compilation {
            status: UIExecutionStatus::Skipped,
            file: std::path::PathBuf::from("lalal"),
        })
        .unwrap();
    use crate::task_types::TaskFormat;
    let path = std::path::Path::new("../task-maker/python/tests/task_with_st");
    let task = task_types::ioi::formats::IOIItalianYaml::parse(path).unwrap();

    let cwd = tempdir::TempDir::new("task-maker").unwrap();
    let store_path = cwd.path().join("store");
    let file_store = FileStore::new(&store_path).expect("Cannot create the file store");
    let executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
    task.evaluate(eval, &IOIEvaluationOptions {}, executor);

    while let Ok(mex) = deserialize_from::<UIMessage>(&receiver) {
        info!("UI: {:?}", mex);
    }
}
