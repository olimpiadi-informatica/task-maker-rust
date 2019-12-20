use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_invalid_shebang(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed]);
}

#[test]
fn with_invalid_shebang_local() {
    better_panic::install();

    with_invalid_shebang(TestInterface::run_local("with_invalid_shebang"));
}

#[test]
fn with_invalid_shebang_remote() {
    better_panic::install();

    with_invalid_shebang(TestInterface::run_remote("with_invalid_shebang"));
}
