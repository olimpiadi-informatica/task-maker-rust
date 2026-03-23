use task_maker_format::ioi::TestcaseEvaluationStatus::*;

mod common;
use common::TestInterface;

fn interactive(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .must_compile("solution.cpp")
        .must_compile("wrong.cpp")
        .must_compile("sleep.cpp")
        .must_compile("tle.cpp")
        .must_compile("crash.cpp")
        .must_compile("wrong_protocol.cpp")
        .solution_score("solution.cpp", vec![100.0])
        .solution_score("wrong.cpp", vec![0.0])
        .solution_score("sleep.cpp", vec![0.0])
        .solution_score("tle.cpp", vec![0.0])
        .solution_score("crash.cpp", vec![0.0])
        .solution_score("wrong_protocol.cpp", vec![0.0])
        .solution_statuses("solution.cpp", vec![Accepted("Ok!".into())])
        .solution_statuses("wrong.cpp", vec![WrongAnswer("Ko!".into())])
        .solution_statuses("sleep.cpp", vec![WallTimeLimitExceeded])
        .solution_statuses("tle.cpp", vec![TimeLimitExceeded])
        .solution_statuses(
            "crash.cpp",
            vec![WrongAnswer("Ko1! maybe caused by runtime error".into())],
        )
        .solution_statuses("wrong_protocol.cpp", vec![WrongAnswer("Ko1!".into())])
        .file_exists("check/controller");
}

fn interactive_many(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![100.0])
        .must_compile("solution.cpp")
        .must_compile("wrong.cpp")
        .must_compile("sleep.cpp")
        .must_compile("tle.cpp")
        .must_compile("crash.cpp")
        .must_compile("early_wa.cpp")
        .solution_score("solution.cpp", vec![100.0])
        .solution_score("wrong.cpp", vec![0.0])
        .solution_score("sleep.cpp", vec![0.0])
        .solution_score("tle.cpp", vec![0.0])
        .solution_score("crash.cpp", vec![0.0])
        .solution_score("early_wa.cpp", vec![0.0])
        .solution_statuses("solution.cpp", vec![Accepted("Output is correct".into())])
        .solution_statuses(
            "wrong.cpp",
            vec![WrongAnswer(
                "Output is incorrect (Admin-only message: wrong output at step 19)".into(),
            )],
        )
        .solution_statuses("sleep.cpp", vec![WallTimeLimitExceeded])
        .solution_statuses("tle.cpp", vec![TimeLimitExceeded])
        .solution_statuses("crash.cpp", vec![RuntimeError])
        .solution_statuses(
            "early_wa.cpp",
            vec![WrongAnswer(
                "Output is incorrect (Admin-only message: wrong output at step 0) maybe caused by runtime error".into(),
            )],
        )
        .file_exists("check/controller");
}

#[test]
fn interactive_many_local() {
    better_panic::install();

    interactive_many(TestInterface::run_local("interactive_many"));
}

#[test]
fn interactive_many_remote() {
    better_panic::install();

    interactive_many(TestInterface::run_remote("interactive_many"));
}

#[test]
fn interactive_local() {
    better_panic::install();

    interactive(TestInterface::run_local("interactive"));
}

#[test]
fn interactive_remote() {
    better_panic::install();

    interactive(TestInterface::run_remote("interactive"));
}
