use task_maker_format::ioi::TestcaseEvaluationStatus::*;

mod common;
use common::TestInterface;

fn communication_stdio(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .must_compile("solution.cpp")
        .must_compile("solution.c")
        .must_compile("no_output.cpp")
        .must_compile("wrong.cpp")
        .solution_score("solution.cpp", vec![100.0])
        .solution_score("solution.c", vec![100.0])
        .solution_score("solution.py", vec![100.0])
        .solution_score("no_output.cpp", vec![0.0])
        .solution_score("wrong.cpp", vec![0.0])
        .solution_statuses("solution.cpp", vec![Accepted("Ok!".into())])
        .solution_statuses("solution.c", vec![Accepted("Ok!".into())])
        .solution_statuses("solution.py", vec![Accepted("Ok!".into())])
        .solution_statuses("no_output.cpp", vec![WrongAnswer("Ko!".into())])
        .solution_statuses("wrong.cpp", vec![WrongAnswer("Ko!".into())])
        .file_exists("check/manager");
}

#[test]
fn communication_stdio_local() {
    better_panic::install();

    communication_stdio(TestInterface::run_local("communication_stdio"));
}

#[test]
fn communication_stdio_remote() {
    better_panic::install();

    communication_stdio(TestInterface::run_remote("communication_stdio"));
}
