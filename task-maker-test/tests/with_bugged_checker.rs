use task_maker_test::*;

#[test]
fn with_bugged_checker_local() {
    better_panic::install();

    TestInterface::run_local("with_bugged_checker").fail("Invalid score from checker");
}

#[test]
fn with_bugged_checker_remote() {
    better_panic::install();

    TestInterface::run_remote("with_bugged_checker").fail("Invalid score from checker");
}
