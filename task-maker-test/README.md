# task-maker tests

In this crate there are some tasks that will be run with task-maker and it's checked that the output they produce it's valid.
Adding a new test is pretty simple, follow those steps:

1. Add a new folder with the test case inside `tasks`
2. Add inside `tests` a `.rs` file with the test, look to another one for inspiration

## task-maker-test-sandbox

Since the sandbox needs to fork+exec in order to work correctly a second binary with the sandbox implementation is required.
Normally task-maker executes itself with different arguments, but this cannot be done because the test binary cannot be customized.

The sandbox binary is built by the `build.rs` script, as a separate crate and in a different target directory, to avoid deadlocking the current one (used by cargo running the build script).