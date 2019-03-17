mod client;
mod executor;
mod file_transmission;
mod local_executor;
mod sandbox;
mod scheduler;
mod signals;
mod worker;

use failure::Error;
use std::sync::mpsc::{Receiver, Sender};

pub use client::*;
pub use executor::*;
pub use file_transmission::*;
pub use local_executor::*;
pub use sandbox::*;
pub use signals::*;
pub use worker::*;

/// The channel part that sends data.
pub type ChannelSender = Sender<String>;
/// The channel part that receives data.
pub type ChannelReceiver = Receiver<String>;

/// Serialize a message into the sender serializing it.
pub fn serialize_into<T>(what: &T, sender: &ChannelSender) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender
        .send(serde_json::to_string(what)?)
        .map_err(|e| e.into())
}

/// Deserialize a message from the channel and return it.
pub fn deserialize_from<T>(reader: &ChannelReceiver) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    serde_json::from_str(&data).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use crate::evaluation::*;
    use crate::execution::*;
    use crate::test_utils::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_local_evaluation() {
        let cwd = setup_test();
        let (mut eval, _receiver) = EvaluationData::new();

        let file = File::new("Input file");

        let mut exec = Execution::new(
            "An execution",
            ExecutionCommand::System(PathBuf::from("true")),
        );
        exec.stdin(&file);
        let stdout = exec.stdout();

        let mut exec2 = Execution::new("Nope!", ExecutionCommand::System(PathBuf::from("false")));
        exec2.stdin(&stdout);
        let stdout2 = exec2.stdout();

        let mut exec3 = Execution::new("Skippp", ExecutionCommand::System(PathBuf::from("true")));
        exec3.stdin(&stdout2);
        let output3 = exec3.output(Path::new("test"));

        let exec_done = Arc::new(AtomicBool::new(false));
        let exec_done2 = exec_done.clone();
        let exec_start = Arc::new(AtomicBool::new(false));
        let exec_start2 = exec_start.clone();
        let exec2_done = Arc::new(AtomicBool::new(false));
        let exec2_done2 = exec2_done.clone();
        let exec2_start = Arc::new(AtomicBool::new(false));
        let exec2_start2 = exec2_start.clone();
        let exec3_skipped = Arc::new(AtomicBool::new(false));
        let exec3_skipped2 = exec3_skipped.clone();
        eval.dag.provide_file(file, Path::new("/dev/null"));
        eval.dag.on_execution_done(&exec.uuid, move |_res| {
            exec_done.store(true, Ordering::Relaxed)
        });
        eval.dag
            .on_execution_skip(&exec.uuid, || assert!(false, "exec has been skipped"));
        eval.dag.on_execution_start(&exec.uuid, move |_w| {
            exec_start.store(true, Ordering::Relaxed)
        });
        eval.dag.add_execution(exec);
        eval.dag.on_execution_done(&exec2.uuid, move |_res| {
            exec2_done.store(true, Ordering::Relaxed)
        });
        eval.dag
            .on_execution_skip(&exec2.uuid, || assert!(false, "exec2 has been skipped"));
        eval.dag.on_execution_start(&exec2.uuid, move |_w| {
            exec2_start.store(true, Ordering::Relaxed)
        });
        eval.dag.add_execution(exec2);
        eval.dag.on_execution_done(&exec3.uuid, |_res| {
            assert!(false, "exec3 has not been skipped")
        });
        eval.dag.on_execution_skip(&exec3.uuid, move || {
            exec3_skipped.store(true, Ordering::Relaxed)
        });
        eval.dag.on_execution_start(&exec3.uuid, |_w| {
            assert!(false, "exec3 has not been skipped")
        });
        eval.dag.add_execution(exec3);
        eval.dag.write_file_to(&stdout, &cwd.path().join("stdout"));
        eval.dag
            .write_file_to(&stdout2, &cwd.path().join("stdout2"));
        eval.dag
            .write_file_to(&output3, &cwd.path().join("output3"));

        eval_dag_locally(eval, cwd.path());

        assert!(exec_done2.load(Ordering::Relaxed));
        assert!(exec_start2.load(Ordering::Relaxed));
        assert!(exec2_done2.load(Ordering::Relaxed));
        assert!(exec2_start2.load(Ordering::Relaxed));
        assert!(exec3_skipped2.load(Ordering::Relaxed));
        assert!(cwd.path().join("stdout").exists());
        assert!(!cwd.path().join("stdout2").exists());
        assert!(!cwd.path().join("output3").exists());
    }
}
