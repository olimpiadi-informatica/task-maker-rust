extern crate serde;
extern crate serde_json;
extern crate uuid;
#[macro_use]
extern crate log;
extern crate env_logger;

mod execution;
mod executor;

use execution::*;
use executor::ExecutorTrait;
use std::sync::mpsc::channel;
use std::thread;

fn main() {
    env_logger::init();
    let mut dag = ExecutionDAG::new();

    let file = File::new("Source file of generator.cpp");
    let lib = File::new("Library for generator.cpp");
    let mut exec = Execution::new(
        "Compilation of generator.cpp",
        ExecutionCommand::System("g++".to_owned()),
    );
    exec.stdin(&file).input(&lib, "lib.h", false);

    let out = exec.stdout();
    let mut exec2 = Execution::new("Loop!!", ExecutionCommand::System("g++".to_owned()));
    exec2.stdin(&out);
    dag.add_execution(exec2)
        .on_done(&|res| warn!("exec2 completed {:?}", res))
        .on_skip(&|| warn!("Skipped execution!!!!"));

    dag.provide_file(lib);
    dag.provide_file(file);
    dag.add_execution(exec)
        .on_start(&|w| warn!("Started on {}!", w))
        .on_done(&|res| warn!("Exec result {:?}", res))
        .write_stderr_to("/tmp/stderr")
        .write_output_to("a.out", "/tmp/output")
        .get_output_content("a.out", 100, &|content| println!("Content: {:?}", content))
        .get_stderr_content(100, &|content| println!("Content: {:?}", content));

    trace!("{:#?}", dag);

    let (tx, rx_remote) = channel();
    let (tx_remote, rx) = channel();

    let server = thread::spawn(move || {
        let mut executor = executor::LocalExecutor::new(1);
        executor.evaluate(tx_remote, rx_remote).unwrap();
    });
    executor::ExecutorClient::evaluate(dag, tx, rx).unwrap();
    server.join().expect("Server paniced");
}
