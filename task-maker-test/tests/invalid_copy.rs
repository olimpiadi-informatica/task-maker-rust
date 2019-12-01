use task_maker_test::*;

#[test]
fn invalid_copy_local() {
    better_panic::install();

    TestInterface::new("invalid_copy")
        .fail("COPY from not existing file")
        .run_local();
}

#[test]
fn invalid_copy_remote() {
    better_panic::install();

    TestInterface::new("invalid_copy")
        .fail("COPY from not existing file")
        .run_remote();
}
