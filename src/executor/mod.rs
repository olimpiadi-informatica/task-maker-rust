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
    use super::*;
    use crate::execution::*;
    use crate::store::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn test_local_evaluation() {
        let cwd = tempdir::TempDir::new("tm-test").unwrap();
        let mut dag = ExecutionDAG::new();

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

        dag.provide_file(file, Path::new("/dev/null"));
        dag.add_execution(exec)
            .on_done(move |_res| exec_done.store(true, Ordering::Relaxed))
            .on_skip(|| assert!(false, "exec has been skipped"))
            .on_start(move |_w| exec_start.store(true, Ordering::Relaxed));
        dag.add_execution(exec2)
            .on_done(move |_res| exec2_done.store(true, Ordering::Relaxed))
            .on_skip(|| assert!(false, "exec2 has been skipped"))
            .on_start(move |_w| exec2_start.store(true, Ordering::Relaxed));
        dag.add_execution(exec3)
            .on_done(|_res| assert!(false, "exec3 has not been skipped"))
            .on_skip(move || exec3_skipped.store(true, Ordering::Relaxed))
            .on_start(|_w| assert!(false, "exec3 has not been skipped"));
        dag.write_file_to(&stdout, &cwd.path().join("stdout"));
        dag.write_file_to(&stdout2, &cwd.path().join("stdout2"));
        dag.write_file_to(&output3, &cwd.path().join("output3"));

        let (tx, rx_remote) = channel();
        let (tx_remote, rx) = channel();

        let server = thread::spawn(move || {
            let file_store =
                FileStore::new(Path::new("/tmp/store")).expect("Cannot create the file store");
            let mut executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
            executor.evaluate(tx_remote, rx_remote).unwrap();
        });
        ExecutorClient::evaluate(dag, tx, rx).unwrap();
        server.join().expect("Server paniced");

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
