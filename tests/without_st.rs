mod common;
use common::TestInterface;
use task_maker_format::ioi::TestcaseEvaluationStatus::*;

fn without_st(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .not_compiled("soluzione.py")
        .not_compiled("40_points.py")
        .solution_score("soluzione.py", vec![100.0])
        .solution_score("40_points.py", vec![40.0])
        .solution_statuses("soluzione.py", vec![Accepted("Output is correct".into())])
        .solution_statuses(
            "40_points.py",
            vec![
                Accepted("Output is correct".into()),
                Accepted("Output is correct".into()),
                WrongAnswer("Output is incorrect".into()),
                WrongAnswer("Output is incorrect".into()),
                WrongAnswer("Output is incorrect".into()),
            ],
        );
}

#[test]
fn without_st_local() {
    better_panic::install();

    without_st(TestInterface::run_local("without_st"));
}

#[test]
fn without_st_remote() {
    better_panic::install();

    without_st(TestInterface::run_remote("without_st"));
}
