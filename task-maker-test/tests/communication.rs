use task_maker_format::ioi::TestcaseEvaluationStatus::*;
use task_maker_test::*;

fn communication(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .must_compile("solution.cpp")
        .must_compile("solution.c")
        .must_compile("wrong.cpp")
        .solution_score("solution.cpp", vec![100.0])
        .solution_score("solution.c", vec![100.0])
        .solution_score("wrong.cpp", vec![0.0])
        .solution_statuses("solution.cpp", vec![Accepted("Ok!".into())])
        .solution_statuses("wrong.cpp", vec![WrongAnswer("Ko!".into())]);
}

#[test]
fn communication_local() {
    better_panic::install();

    communication(TestInterface::run_local("communication"));
}

#[test]
fn communication_remote() {
    better_panic::install();

    communication(TestInterface::run_remote("communication"));
}
