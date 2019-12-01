use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_invalid_shebang() -> TestInterface {
    let mut test_interface = TestInterface::new("with_invalid_shebang");
    test_interface
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed]);
    test_interface
}

#[test]
fn with_invalid_shebang_local() {
    better_panic::install();

    with_invalid_shebang().run_local();
}

#[test]
fn with_invalid_shebang_remote() {
    better_panic::install();

    with_invalid_shebang().run_remote();
}
