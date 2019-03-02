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
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();

    let mut dag = ExecutionDAG::new();

    let file = File::new("Source file of generator.cpp");
    let lib = File::new("Library for generator.cpp");
    let mut exec = Execution::new(
        "Compilation of generator.cpp",
        ExecutionCommand::System("g++".to_owned()),
    );
    exec.stdin(&file).input(&lib, "lib.h", false);

    let stdout = exec.stdout();

    dag.add_execution(exec)
        .on_start(&|w| warn!("Started on {}!", w))
        .on_done(&|res| warn!("Exec result {:?}", res))
        .write_stderr_to("/tmp/stderr")
        .write_output_to("a.out", "/tmp/output")
        .get_output_content("a.out", 100, &|content| warn!("Content: {:?}", content))
        .get_stderr_content(100, &|content| warn!("Content: {:?}", content));

    for i in 0..1 {
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

    dag.provide_file(lib);
    dag.provide_file(file);

    trace!("{:#?}", dag);

    let (tx, rx_remote) = channel();
    let (tx_remote, rx) = channel();

    let server = thread::spawn(move || {
        let mut executor = executor::LocalExecutor::new(2);
        executor.evaluate(tx_remote, rx_remote).unwrap();
    });
    executor::ExecutorClient::evaluate(dag, tx, rx).unwrap();
    server.join().expect("Server paniced");
}
