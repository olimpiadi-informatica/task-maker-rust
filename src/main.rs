extern crate serde;
extern crate serde_json;
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

pub mod execution;
pub mod executor;
pub mod format;
pub mod store;

use execution::*;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use store::*;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    format::ioi::parse();

    let mut dag = ExecutionDAG::new();

    let file = File::new("Source file of generator.cpp");
    let lib = File::new("Library for generator.cpp");
    let mut exec = Execution::new(
        "Compilation of generator.cpp",
        ExecutionCommand::System("g++".to_owned()),
    );
    let mut limits = ExecutionLimits::default();
    limits.cpu_time(2.0);
    exec.stdin(&file)
        .input(&lib, Path::new("test/nested/dir/lib.h"), true)
        .limits(limits);

    let stdout = exec.stdout();
    dag.write_file_to(&stdout, Path::new("/tmp/stdout"));
    dag.write_file_to(&exec.stderr(), Path::new("/tmp/stderr"));
    dag.write_file_to(&exec.output(Path::new("a.out")), Path::new("/tmp/output"));
    dag.get_file_content(&exec.output(Path::new("a.out")), 100, |content| {
        warn!("a.out: {:?}", content)
    });
    dag.add_execution(exec)
        .on_start(move |w| warn!("Started on {}!", w))
        .on_done(move |res| warn!("Exec result {:?}", res));

    for i in 0..10 {
        let mut exec = Execution::new(
            &format!("Execution {}", i),
            ExecutionCommand::System("g++".to_owned()),
        );
        exec.stdin(&stdout);
        dag.add_execution(exec)
            .on_done(&|_res| warn!("Done!"))
            .on_skip(&|| warn!("Skipped!"));
    }

    let mut exec2 = Execution::new("Loop!!", ExecutionCommand::System("g++".to_owned()));
    exec2.stdin(&stdout);
    let stdout2 = exec2.stdout();
    dag.add_execution(exec2)
        .on_done(&|res| warn!("exec2 completed {:?}", res))
        .on_skip(&|| warn!("Skipped execution!!!!"));

    let mut exec3 = Execution::new("lalal", ExecutionCommand::System("kakak".to_owned()));
    exec3.stdin(&stdout2);
    dag.add_execution(exec3);

    dag.provide_file(lib, Path::new("/dev/null"));
    dag.provide_file(file, Path::new("/dev/null"));

    trace!("{:#?}", dag);

    let (tx, rx_remote) = channel();
    let (tx_remote, rx) = channel();

    let server = thread::spawn(move || {
        let file_store =
            FileStore::new(Path::new("/tmp/store")).expect("Cannot create the file store");
        let mut executor = executor::LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
        executor.evaluate(tx_remote, rx_remote).unwrap();
    });
    executor::ExecutorClient::evaluate(dag, tx, rx).unwrap();
    server.join().expect("Server paniced");
}
