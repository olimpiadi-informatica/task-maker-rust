use task_maker_test::*;

#[test]
fn invalid_copy_local() {
    better_panic::install();

    TestInterface::run_local("invalid_copy").fail("COPY from not existing file");
}

#[test]
fn invalid_copy_remote() {
    better_panic::install();

    TestInterface::run_remote("invalid_copy").fail("COPY from not existing file");
}
