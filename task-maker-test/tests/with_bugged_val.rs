use task_maker_format::ioi::TestcaseEvaluationStatus::Skipped;
use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_bugged_val(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .validation_fails(vec![Some("assert False".into())])
        .solution_statuses("soluzione.cpp", vec![Skipped]);
}

#[test]
fn with_bugged_val_local() {
    better_panic::install();

    with_bugged_val(TestInterface::run_local("with_bugged_val"));
}

#[test]
fn with_bugged_val_remote() {
    better_panic::install();

    with_bugged_val(TestInterface::run_remote("with_bugged_val"));
}
