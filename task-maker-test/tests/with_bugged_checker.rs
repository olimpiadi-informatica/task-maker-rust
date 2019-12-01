use task_maker_test::*;

#[test]
fn with_bugged_checker_local() {
    better_panic::install();

    TestInterface::new("with_bugged_checker")
        .fail("Invalid score from checker")
        .run_local();
}

#[test]
fn with_bugged_checker_remote() {
    better_panic::install();

    TestInterface::new("with_bugged_checker")
        .fail("Invalid score from checker")
        .run_remote();
}
