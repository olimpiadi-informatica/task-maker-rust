use task_maker_format::ioi::TestcaseEvaluationStatus::*;
use task_maker_test::*;

#[test]
fn with_checker() {
    better_panic::install();

    TestInterface::new("with_checker")
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .not_compiled("soluzione.sh")
        .not_compiled("wrong.sh")
        .solution_score("soluzione.sh", vec![100.0])
        .solution_score("wrong.sh", vec![0.0])
        .solution_statuses("soluzione.sh", vec![Accepted("Ok!".into())])
        .solution_statuses("wrong.sh", vec![WrongAnswer("Ko!".into())])
        .run();
}
