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
        .input(src1.uuid, "script.sh", true);
    exec1.capture_stdout(Some(1000));
    exec1.capture_stderr(Some(1000));
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
    group.add_execution(exec1);

    // exec2 will read from fifo1
    // then write it back into fifo2
    let mut exec2 = Execution::new("exec2", ExecutionCommand::local("script.sh"));
    let src2 = File::new("source 2");
    exec2
        .args(vec![fifo1, fifo2])
        .input(src2.uuid, "script.sh", true);
    exec2.capture_stdout(Some(1000));
    exec2.capture_stderr(Some(1000));
    exec2.limits_mut().wall_time(3.0).allow_multiprocess();
    dag.provide_content(
        src2,
        "#!/usr/bin/env bash\n\
            res=$(cat $1)\n\
            echo $res > $2\n"
            .as_bytes()
            .to_owned(),
    );
    group.add_execution(exec2);

    let group_uuid = group.uuid;
    dag.on_execution_done(&group_uuid, |res| {
        let res1 = &res[0];
        let res2 = &res[1];
        assert!(res1.status.is_success(), "Process 1 crashed: {res1:?}");
        assert!(res2.status.is_success(), "Process 2 crashed: {res2:?}");
        Ok(())
    });
    dag.on_execution_skip(&group_uuid, || panic!("Group has been skipped"));

    dag.add_execution_group(group);
    eval_dag(dag);
}
