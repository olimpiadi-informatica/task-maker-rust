mod common;
use common::{eval_dag, setup};
use task_maker_dag::{Execution, ExecutionCommand, ExecutionDAG, ExecutionGroup};

#[test]
fn test_remove_output_file() {
    setup();

    let mut dag = ExecutionDAG::new();
    let mut exec = Execution::new("exec", ExecutionCommand::system("rm"));
    exec.args(vec!["file1"])
        .capture_stdout(1000)
        .capture_stderr(1000)
        .output("file1");

    dag.on_execution_done(&exec.uuid, |res| {
        assert!(!res.status.is_success(), "rm didn't fail: {res:?}");
        Ok(())
    });
    dag.add_execution(exec);
    eval_dag(dag);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn test_chmod_dir() {
    setup();

    let mut dag = ExecutionDAG::new();
    let mut exec = Execution::new("exec", ExecutionCommand::system("chmod"));
    exec.args(vec!["777", "."])
        .capture_stdout(1000)
        .capture_stderr(1000)
        .output("file1");

    dag.on_execution_done(&exec.uuid, |res| {
        assert!(!res.status.is_success(), "chmod didn't fail: {res:?}");
        Ok(())
    });
    dag.add_execution(exec);
    eval_dag(dag);
}

#[test]
fn test_create_files() {
    setup();

    let mut dag = ExecutionDAG::new();
    let mut exec = Execution::new("exec", ExecutionCommand::system("touch"));
    exec.args(vec!["lolnope"])
        .capture_stdout(1000)
        .capture_stderr(1000);

    dag.on_execution_done(&exec.uuid, |res| {
        assert!(!res.status.is_success(), "touch didn't fail: {res:?}");
        Ok(())
    });
    dag.add_execution(exec);
    eval_dag(dag);
}

#[test]
fn test_list_fifo() {
    setup();

    let mut dag = ExecutionDAG::new();
    let mut group = ExecutionGroup::new("group");
    let fifo = group.new_fifo();
    let fifo_dir = fifo.sandbox_path().parent().unwrap().to_owned();
    group.new_fifo();
    let mut exec = Execution::new("exec", ExecutionCommand::system("ls"));
    exec.args(vec![fifo_dir.to_str().unwrap()])
        .capture_stdout(1000)
        .capture_stderr(1000)
        .output("file1");

    dag.on_execution_done(&exec.uuid, |res| {
        assert!(!res.status.is_success(), "ls didn't fail: {res:?}");
        Ok(())
    });
    dag.add_execution(exec);
    eval_dag(dag);
}
