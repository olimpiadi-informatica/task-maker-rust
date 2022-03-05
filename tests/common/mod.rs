#![allow(dead_code)]

pub use test_interface::*;

mod test_interface;

use task_maker_dag::ExecutionDAG;
use task_maker_exec::eval_dag_locally;
use task_maker_rust::ToolsSandboxRunner;

pub fn setup() {
    let _ = env_logger::Builder::from_default_env()
        .format_timestamp_nanos()
        .is_test(true)
        .try_init();
    std::env::set_var(
        "TASK_MAKER_TOOLS_PATH",
        env!("CARGO_BIN_EXE_task-maker-tools"),
    );
}

pub fn eval_dag(dag: ExecutionDAG) {
    let cwd = tempdir::TempDir::new("tm-test").unwrap();
    eval_dag_locally(
        dag,
        cwd.path(),
        2,
        cwd.path(),
        1000,
        1000,
        ToolsSandboxRunner::default(),
    );
}
