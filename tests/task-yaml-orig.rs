mod common;
use common::TestInterface;

fn classic(test: TestInterface) {
    test.success()
        .time_limit(1.0)
        .memory_limit(64)
        .max_score(100.0)
        .subtask_scores(vec![5.0, 45.0, 50.0])
        .must_compile("generatore.cpp")
        .must_compile("solution.cpp")
        .must_compile("wrong_file.cpp")
        .solution_score("solution.cpp", vec![5.0, 45.0, 50.0])
        .solution_score("wrong_file.cpp", vec![0.0, 0.0, 0.0]);
}

#[test]
fn classic_local() {
    better_panic::install();
    classic(TestInterface::run_local("task-yaml-orig"));
}

#[test]
fn classic_remote() {
    better_panic::install();
    classic(TestInterface::run_remote("task-yaml-orig"));
}
