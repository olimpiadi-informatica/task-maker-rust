use std::io::{stdin, stdout};
use tabox::{Sandbox, SandboxImplementation};
use task_maker_exec::RawSandboxResult;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .init();
    let config = serde_json::from_reader(stdin()).unwrap();
    let sandbox = SandboxImplementation::run(config).unwrap();
    let res = sandbox.wait().unwrap();
    serde_json::to_writer(stdout(), &RawSandboxResult::Success(res)).unwrap();
}
