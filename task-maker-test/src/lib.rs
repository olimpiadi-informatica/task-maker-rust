#[macro_use]
extern crate approx;

pub use test_interface::*;
#[cfg(test)]
pub use tests::*;

mod fifo;
mod sandbox;
mod test_interface;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use task_maker_dag::ExecutionDAG;
    use task_maker_exec::eval_dag_locally;
    use task_maker_rust::SelfExecSandboxRunner;

    pub fn setup() {
        let _ = env_logger::Builder::from_default_env()
            .default_format_timestamp_nanos(true)
            .is_test(true)
            .try_init();
        std::env::set_var(
            "TASK_MAKER_SANDBOX_BIN",
            PathBuf::from(env!("OUT_DIR")).join("sandbox"),
        );
    }

    pub fn eval_dag(dag: ExecutionDAG) {
        let cwd = tempdir::TempDir::new("tm-test").unwrap();
        eval_dag_locally(
            dag,
            cwd.path(),
            2,
            cwd.path(),
            1000,
            1000,
            SelfExecSandboxRunner::default(),
        );
    }
}
