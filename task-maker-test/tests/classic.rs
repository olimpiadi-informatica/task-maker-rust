use task_maker_format::ioi::TestcaseEvaluationStatus::*;
use task_maker_test::*;

fn classic(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![5.0, 45.0, 50.0])
        .must_compile("generatore.cpp")
        .must_compile("mle.cpp")
        .must_compile("float_error.cpp")
        .must_compile("nonzero.cpp")
        .must_compile("sigsegv.c")
        .must_compile("tle.cpp")
        .must_compile("wa.cpp")
        .must_compile("wrong_file.cpp")
        .must_not_compile("not_compile.cpp")
        // .not_compiled(".ignoreme.cpp")
        .not_compiled("bash.sh")
        .not_compiled("noop.py")
        .not_compiled("soluzione.py")
        .solution_score("soluzione.py", vec![5.0, 45.0, 50.0])
        .solution_score("bash.sh", vec![5.0, 45.0, 50.0])
        .solution_score("float_error.cpp", vec![0.0, 0.0, 0.0])
        .solution_score("mle.cpp", vec![0.0, 0.0, 0.0])
        .solution_score("noop.py", vec![0.0, 0.0, 0.0])
        .solution_score("nonzero.cpp", vec![0.0, 0.0, 0.0])
        .solution_score("sigsegv.c", vec![0.0, 0.0, 0.0])
        .solution_score("tle.cpp", vec![5.0, 45.0, 0.0])
        .solution_score("wa.cpp", vec![0.0, 0.0, 0.0])
        .solution_score("wrong_file.cpp", vec![0.0, 0.0, 0.0])
        .solution_statuses("soluzione.py", vec![Accepted("Output is correct".into())])
        .solution_statuses("bash.sh", vec![Accepted("Output is correct".into())])
        // .solution_statuses("mle.cpp", vec![RuntimeError]) // pretty unreliable
        .solution_statuses("nonzero.cpp", vec![RuntimeError])
        .solution_statuses("sigsegv.c", vec![RuntimeError])
        .solution_statuses(
            "tle.cpp",
            vec![
                Accepted("Output is correct".into()),
                Accepted("Output is correct".into()),
                Accepted("Output is correct".into()),
                Accepted("Output is correct".into()),
                TimeLimitExceeded,
                TimeLimitExceeded,
            ],
        )
        .solution_statuses("wa.cpp", vec![WrongAnswer("Output is incorrect".into())])
        .solution_statuses(
            "wrong_file.cpp",
            vec![WrongAnswer("Output is incorrect".into())],
        );
}

#[test]
fn classic_local() {
    better_panic::install();
    classic(TestInterface::run_local("classic"));
}

#[test]
fn classic_remote() {
    better_panic::install();
    classic(TestInterface::run_remote("classic"));
}
