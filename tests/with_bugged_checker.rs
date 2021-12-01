mod common;
use common::TestInterface;

#[test]
fn with_bugged_checker_local() {
    better_panic::install();

    TestInterface::run_local("with_bugged_checker").fail("Invalid score");
}

#[test]
fn with_bugged_checker_remote() {
    better_panic::install();

    TestInterface::run_remote("with_bugged_checker").fail("Invalid score");
}
