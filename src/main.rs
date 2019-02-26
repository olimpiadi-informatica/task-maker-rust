extern crate bincode;
extern crate serde;
extern crate uuid;

mod execution;

use execution::*;
use std::rc::Rc;

fn main() {
    let mut dag = ExecutionDAG::new();
    let file = File::new("Source file of generator.cpp");
    let lib = File::new("Library for generator.cpp");
    let mut exec = Execution::new(
        "Compilation of generator.cpp",
        ExecutionCommand::System("/usr/bin/g++".to_owned()),
    );
    exec.stdin(file.clone())
        .input(lib, "lib.h", false)
        .on_start(&|| println!("Started!"));

    let stdout = exec.stdout();
    let output = exec.output("a.out");

    output
        .lock()
        .expect("Cannot lock2")
        .get_content(10000, &|data| println!("data: {:?}", data));
    let exec = Rc::new(exec);
    dag.provide_file(file.clone());
    dag.add_execution(exec.clone());

    println!("dag: {:#?}\n", dag);
    println!("stdout: {:#?}\n", stdout);
    println!("output: {:#?}\n", output);
    dag.execute();
}
