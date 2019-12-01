use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_bugged_gen() -> TestInterface {
    let mut test_interface = TestInterface::new("with_bugged_gen");
    test_interface
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .generation_fails(vec![Some(":(".into())]);
    test_interface
}

#[test]
fn with_bugged_gen_local() {
    better_panic::install();

    with_bugged_gen().run_local();
}

#[test]
fn with_bugged_gen_remote() {
    better_panic::install();

    with_bugged_gen().run_remote();
}
