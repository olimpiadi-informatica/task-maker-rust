mod common;
use common::TestInterface;

fn with_bugged_checker(test: TestInterface) {
    test.success()
        .has_diagnostic("Checker returned an invalid score");
}

#[test]
fn with_bugged_checker_local() {
    better_panic::install();

    with_bugged_checker(TestInterface::run_local("with_bugged_checker"));
}

#[test]
fn with_bugged_checker_remote() {
    better_panic::install();

    with_bugged_checker(TestInterface::run_remote("with_bugged_checker"));
}
