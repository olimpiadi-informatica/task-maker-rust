use task_maker_test::*;

#[test]
fn invalid_copy() {
    better_panic::install();

    TestInterface::new("invalid_copy")
        .fail("COPY from not existing file")
        .run();
}
