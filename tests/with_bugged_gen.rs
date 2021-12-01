mod common;
use common::TestInterface;

use task_maker_format::ioi::TestcaseGenerationStatus::Failed;

fn with_bugged_gen(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .generation_fails(vec![Some(":(".into())]);
}

#[test]
fn with_bugged_gen_local() {
    better_panic::install();

    with_bugged_gen(TestInterface::run_local("with_bugged_gen"));
}

#[test]
fn with_bugged_gen_remote() {
    better_panic::install();

    with_bugged_gen(TestInterface::run_remote("with_bugged_gen"));
}
