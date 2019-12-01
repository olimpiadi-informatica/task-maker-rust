use task_maker_format::ioi::TestcaseEvaluationStatus::Skipped;
use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_bugged_val() -> TestInterface {
    let mut test_interface = TestInterface::new("with_bugged_val");
    test_interface
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .validation_fails(vec![Some("assert False".into())])
        .solution_statuses("soluzione.cpp", vec![Skipped]);
    test_interface
}

#[test]
fn _local() {
    better_panic::install();

    with_bugged_val().run_local();
}

#[test]
fn with_bugged_val_remote() {
    better_panic::install();

    with_bugged_val().run_remote();
}
