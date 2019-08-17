use crate::proto::*;
use crate::*;
use failure::Error;
use std::io::Write;
use task_maker_store::*;

/// This is a client of the `Executor`, the client is who sends a DAG for an evaluation, provides
/// some files and receives the callbacks from the server. When the server notifies a callback
/// function is called by the client.
pub struct ExecutorClient;

impl ExecutorClient {
    /// Begin the evaluation sending the DAG to the server, sending the files as needed and storing
    /// the files from the server.
    ///
    /// This method is blocking until the server ends the computation.
    ///
    /// * `eval` - The EvaluationData to evaluate.
    /// * `sender` - A channel that sends messages to the server.
    /// * `receiver` - A channel that receives messages from the server.
    ///
    /// ```
    /// use task_maker_dag::ExecutionDAG;
    /// use task_maker_store::FileStore;
    /// use task_maker_exec::{executors::LocalExecutor, ExecutorClient};
    /// use std::sync::mpsc::channel;
    /// use std::sync::{Arc, Mutex};
    /// use std::thread;
    /// use std::path::PathBuf;
    ///
    /// // make a new, empty, DAG
    /// let dag = ExecutionDAG::new();
    /// // setup the communication channels
    /// let (tx, rx_remote) = channel();
    /// let (tx_remote, rx) = channel();
    /// // make a new local executor in a second thread
    /// let server = thread::spawn(move || {
    ///     let file_store = FileStore::new("/tmp/store").expect("Cannot create the file store");
    ///     let mut executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
    ///     executor.evaluate(tx_remote, rx_remote).unwrap();
    /// });
    ///
    /// ExecutorClient::evaluate(dag, tx, rx).unwrap(); // this will block!
    ///
    /// server.join().expect("Server paniced");
    /// # // cleanup
    /// # std::fs::remove_dir_all("/tmp/store").unwrap();
    /// ```
    pub fn evaluate(
        mut dag: ExecutionDAG,
        sender: ChannelSender,
        receiver: ChannelReceiver,
    ) -> Result<(), Error> {
        trace!("ExecutorClient started");
        // list all the files/executions that want callbacks
        let dag_callbacks = ExecutionDAGWatchSet {
            executions: dag.execution_callbacks.keys().cloned().collect(),
            files: dag.file_callbacks.keys().cloned().collect(),
        };
        let provided_files = dag.data.provided_files.clone();
        serialize_into(
            &ExecutorClientMessage::Evaluate {
                dag: dag.data,
                callbacks: dag_callbacks,
            },
            &sender,
        )?;
        loop {
            match deserialize_from::<ExecutorServerMessage>(&receiver) {
                Ok(ExecutorServerMessage::AskFile(uuid)) => {
                    info!("Server is asking for {}", uuid);
                    let path = &provided_files
                        .get(&uuid)
                        .expect("Server asked for non provided file")
                        .local_path;
                    let key = FileStoreKey::from_file(path)?;
                    serialize_into(&ExecutorClientMessage::ProvideFile(uuid, key), &sender)?;
                    ChannelFileSender::send(&path, &sender)?;
                }
                Ok(ExecutorServerMessage::ProvideFile(uuid)) => {
                    info!("Server sent the file {}", uuid);
                    let iterator = ChannelFileIterator::new(&receiver);
                    if let Some(callback) = dag.file_callbacks.get_mut(&uuid) {
                        let limit = callback
                            .get_content
                            .as_ref()
                            .map(|(limit, _)| *limit)
                            .unwrap_or(0);
                        let mut buffer: Vec<u8> = Vec::new();
                        let mut file = match &callback.write_to {
                            Some(path) => {
                                std::fs::create_dir_all(path.parent().unwrap())?;
                                Some(std::fs::File::create(path)?)
                            }
                            None => None,
                        };
                        for chunk in iterator {
                            if let Some(file) = &mut file {
                                file.write_all(&chunk)?;
                            }
                            if buffer.len() < limit {
                                let len = std::cmp::min(chunk.len(), limit - buffer.len());
                                buffer.extend_from_slice(&chunk[..len]);
                            }
                        }

                        if let Some(get_content) = callback.get_content.take().map(|(_, f)| f) {
                            get_content.call(buffer);
                        }
                    } else {
                        iterator.last();
                    }
                }
                Ok(ExecutorServerMessage::NotifyStart(uuid, worker)) => {
                    info!("Execution {} started on {}", uuid, worker);
                    if let Some(callbacks) = dag.execution_callbacks.get_mut(&uuid) {
                        for callback in callbacks.on_start.drain(..) {
                            callback.call(worker);
                        }
                    }
                }
                Ok(ExecutorServerMessage::NotifyDone(uuid, result)) => {
                    info!("Execution {} completed with {:?}", uuid, result);
                    if let Some(callbacks) = dag.execution_callbacks.get_mut(&uuid) {
                        for callback in callbacks.on_done.drain(..) {
                            callback.call(result.clone());
                        }
                    }
                }
                Ok(ExecutorServerMessage::NotifySkip(uuid)) => {
                    info!("Execution {} skipped", uuid);
                    if let Some(callbacks) = dag.execution_callbacks.get_mut(&uuid) {
                        for callback in callbacks.on_skip.drain(..) {
                            callback.call();
                        }
                    }
                }
                Ok(ExecutorServerMessage::Error(error)) => {
                    info!("Error occurred: {}", error);
                    // TODO abort
                    drop(receiver);
                    break;
                }
                Ok(ExecutorServerMessage::Status(status)) => {
                    info!("Server status: {:#?}", status);
                }
                Ok(ExecutorServerMessage::Done) => {
                    info!("Execution completed!");
                    drop(receiver);
                    break;
                }
                Err(e) => {
                    let cause = e.find_root_cause().to_string();
                    if cause == "receiving on a closed channel" {
                        trace!("Connection closed: {}", cause);
                        break;
                    } else {
                        error!("Connection error: {}", cause);
                    }
                }
            }
        }
        Ok(())
    }
}
