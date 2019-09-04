use task_maker_format::ioi::TestcaseEvaluationStatus::Solved;
use task_maker_format::ioi::TestcaseGenerationStatus::Failed;
use task_maker_test::*;

#[test]
fn with_bugged_sol() {
    better_panic::install();

    TestInterface::new("with_bugged_sol")
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .generation_statuses(vec![Failed])
        .solution_statuses("ok.py", vec![Solved])
        .run();
}
