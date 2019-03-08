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

pub mod execution;
pub mod executor;
pub mod store;
pub mod task_types;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    println!("Tmbox: {}/bin/tmbox", env!("OUT_DIR"));
}
