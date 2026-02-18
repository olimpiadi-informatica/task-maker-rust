mod common;
use common::{eval_dag, setup};
use task_maker_dag::{Execution, ExecutionCommand, ExecutionDAG, ExecutionGroup, File};

#[test]
fn test_fifo() {
    setup();
    let mut dag = ExecutionDAG::new();

    let mut group = ExecutionGroup::new("group");
    let fifo1 = group.new_fifo().sandbox_path();
    let fifo1 = fifo1.to_str().unwrap();
    let fifo2 = group.new_fifo().sandbox_path();
    let fifo2 = fifo2.to_str().unwrap();

    // exec1 will write 42 in fifo1
    // then read it back from fifo2
    // if it is 42 exits with 0, otherwise exit with 1
    let mut exec1 = Execution::new("exec1", ExecutionCommand::local("script.sh"));
    let src1 = File::new("source 1");
    exec1
        .args(vec![fifo1, fifo2])
        .capture_stdout(1000)
        .capture_stderr(1000)
        .input(src1.uuid, "script.sh", true);
    exec1.limits_mut().wall_time(3.0).allow_multiprocess();
    dag.provide_content(
        src1,
        "#!/usr/bin/env bash\n\
            echo 42 > $1\n\
            res=$(cat $2)\n\
            [[ $res == 42 ]] && exit 0 || exit 1\n"
            .as_bytes()
            .to_owned(),
    );
    dag.on_execution_done(&exec1.uuid, |res| {
        assert!(res.status.is_success(), "Process 1 crashed: {res:?}");
        Ok(())
    });
    dag.on_execution_skip(&exec1.uuid, || panic!("Process 1 has been skipped"));
    group.add_execution(exec1);

    // exec2 will read from fifo1
    // then write it back into fifo2
    let mut exec2 = Execution::new("exec2", ExecutionCommand::local("script.sh"));
    let src2 = File::new("source 2");
    exec2
        .args(vec![fifo1, fifo2])
        .capture_stdout(1000)
        .capture_stderr(1000)
        .input(src2.uuid, "script.sh", true);
    exec2.limits_mut().wall_time(3.0).allow_multiprocess();
    dag.provide_content(
        src2,
        "#!/usr/bin/env bash\n\
            res=$(cat $1)\n\
            echo $res > $2\n"
            .as_bytes()
            .to_owned(),
    );
    dag.on_execution_done(&exec2.uuid, |res| {
        assert!(res.status.is_success(), "Process 2 crashed: {res:?}");
        Ok(())
    });
    dag.on_execution_skip(&exec2.uuid, || panic!("Process 2 has been skipped"));
    group.add_execution(exec2);

    dag.add_execution_group(group);
    eval_dag(dag);
}
