use task_maker_format::ioi::TestcaseEvaluationStatus::Solved;
use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

fn with_bugged_sol(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .solution_statuses("ok.py", vec![Solved]);
}

#[test]
fn with_bugged_sol_local() {
    better_panic::install();

    with_bugged_sol(TestInterface::run_local("with_bugged_sol"));
}

#[test]
fn with_bugged_sol_remote() {
    better_panic::install();

    with_bugged_sol(TestInterface::run_remote("with_bugged_sol"));
}
