use task_maker_test::*;

#[test]
fn with_bugged_checker() {
    better_panic::install();

    TestInterface::new("with_bugged_checker")
        .fail("Invalid score from checker")
        .run();
}
