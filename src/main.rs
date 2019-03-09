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
extern crate glob;

pub mod execution;
pub mod executor;
pub mod languages;
pub mod score_types;
pub mod store;
pub mod task_types;
#[cfg(test)]
mod test_utils;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    println!("Tmbox: {}/bin/tmbox", env!("OUT_DIR"));
    use crate::execution::*;
    use crate::task_types::ioi::*;

    let mut dag = ExecutionDAG::new();
    use crate::task_types::TaskFormat;
    let task = task_types::ioi::formats::IOIItalianYaml::parse(std::path::Path::new(
        "../oii/problemi/carestia",
    ))
    .unwrap();
    task.evaluate(&mut dag, &IOIEvaluationOptions {});
    info!("Task: {:#?}", task);
    info!("Dag: {:#?}", dag);
}
