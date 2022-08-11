use task_maker_format::ioi::TestcaseEvaluationStatus::*;

mod common;
use common::TestInterface;

fn communication_with_gen(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![50.0, 50.0])
        .must_compile("solution.cpp")
        .must_compile("solution.c")
        .must_compile("wrong.cpp")
        .solution_score("solution.cpp", vec![50.0, 50.0])
        .solution_score("solution.c", vec![50.0, 50.0])
        .solution_score("wrong.cpp", vec![0.0, 0.0])
        .solution_statuses("solution.cpp", vec![Accepted("Ok!".into())])
        .solution_statuses("wrong.cpp", vec![WrongAnswer("Ko!".into())])
        .file_exists("check/manager");
}

#[test]
fn communication_with_gen_local() {
    better_panic::install();

    communication_with_gen(TestInterface::run_local("communication_with_gen"));
}

#[test]
fn communication_with_gen_remote() {
    better_panic::install();

    communication_with_gen(TestInterface::run_remote("communication_with_gen"));
}
