use crate::proto::*;
use crate::*;
use failure::Error;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use task_maker_dag::{FileCallbacks, FileUuid, ProvidedFile};
use task_maker_store::*;

/// Interval between each Status message is sent asking for server status updates.
const STATUS_POLL_INTERVAL_MS: u64 = 1000;

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
    /// use task_maker_cache::Cache;
    /// # use tempdir::TempDir;
    ///
    /// // make a new, empty, DAG
    /// let dag = ExecutionDAG::new();
    /// // setup the communication channels
    /// let (tx, rx_remote) = channel();
    /// let (tx_remote, rx) = channel();
    /// // make a new local executor in a second thread
    /// let server = thread::spawn(move || {
    /// #   let tmpdir = TempDir::new("tm-test").unwrap();
    /// #   let path = tmpdir.path();
    ///     let file_store = FileStore::new(path).expect("Cannot create the file store");
    ///     let cache = Cache::new(path).expect("Cannot create the cache");
    ///     let mut executor = LocalExecutor::new(Arc::new(file_store), 4, path);
    ///     executor.evaluate(tx_remote, rx_remote, cache).unwrap();
    /// });
    ///
    /// ExecutorClient::evaluate(dag, tx, &rx, |_| {}).unwrap(); // this will block!
    ///
    /// server.join().expect("Server paniced");
    /// ```
    pub fn evaluate<F: FnMut(ExecutorStatus<SystemTime>)>(
        mut dag: ExecutionDAG,
        sender: ChannelSender,
        receiver: &ChannelReceiver,
        mut status_callback: F,
    ) -> Result<(), Error> {
        trace!("ExecutorClient started");
        // list all the files/executions that want callbacks
        let dag_callbacks = ExecutionDAGWatchSet {
            executions: dag.execution_callbacks.keys().cloned().collect(),
            files: dag.file_callbacks.keys().cloned().collect(),
        };
        let provided_files = dag.data.provided_files.clone();
        for (uuid, file) in provided_files.iter() {
            match file {
                ProvidedFile::LocalFile { local_path, .. } => {
                    let iterator = ReadFileIterator::new(&local_path)?;
                    process_provided_file(&mut dag.file_callbacks, *uuid, true, iterator)?;
                }
                ProvidedFile::Content { content, .. } => {
                    process_provided_file(
                        &mut dag.file_callbacks,
                        *uuid,
                        true,
                        vec![content.clone()],
                    )?;
                }
            }
        }
        serialize_into(
            &ExecutorClientMessage::Evaluate {
                dag: dag.data,
                callbacks: dag_callbacks,
            },
            &sender,
        )?;
        // setup the status poller that will send to the server a Status message every
        // STATUS_POLL_INTERVAL_MS milliseconds.
        let done = Arc::new(AtomicBool::new(false));
        let done_thread = done.clone();
        let file_mode = Arc::new(Mutex::new(()));
        let file_mode_thread = file_mode.clone();
        let sender_thread = sender.clone();
        let status_poller = thread::Builder::new()
            .name("Client status poller".into())
            .spawn(move || {
                while !done_thread.load(Ordering::Relaxed) {
                    {
                        // make sure to not interfere with the file sending protocol.
                        let _lock = file_mode_thread.lock().unwrap();
                        // this may fail if the server is gone
                        let _ = serialize_into(&ExecutorClientMessage::Status, &sender_thread);
                    }
                    thread::sleep(Duration::from_millis(STATUS_POLL_INTERVAL_MS));
                }
            })
            .unwrap();
        loop {
            match deserialize_from::<ExecutorServerMessage>(&receiver) {
                Ok(ExecutorServerMessage::AskFile(uuid)) => {
                    info!("Server is asking for {}", uuid);
                    // prevent the status poller for sending messages while sending the file
                    let _lock = file_mode.lock().unwrap();
                    match &provided_files[&uuid] {
                        ProvidedFile::LocalFile {
                            local_path, key, ..
                        } => {
                            serialize_into(
                                &ExecutorClientMessage::ProvideFile(uuid, key.clone()),
                                &sender,
                            )?;
                            ChannelFileSender::send(&local_path, &sender)?;
                        }
                        ProvidedFile::Content { content, key, .. } => {
                            serialize_into(
                                &ExecutorClientMessage::ProvideFile(uuid, key.clone()),
                                &sender,
                            )?;
                            ChannelFileSender::send_data(content.clone(), &sender)?;
                        }
                    }
                }
                Ok(ExecutorServerMessage::ProvideFile(uuid, success)) => {
                    info!("Server sent the file {}, success: {}", uuid, success);
                    let iterator = ChannelFileIterator::new(&receiver);
                    process_provided_file(&mut dag.file_callbacks, uuid, success, iterator)?;
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
                    error!("Error occurred: {}", error);
                    // TODO abort
                    break;
                }
                Ok(ExecutorServerMessage::Status(status)) => {
                    info!("Server status: {:#?}", status);
                    status_callback(ExecutorStatus {
                        connected_workers: status
                            .connected_workers
                            .into_iter()
                            .map(|worker| ExecutorWorkerStatus {
                                uuid: worker.uuid,
                                name: worker.name,
                                current_job: worker
                                    .current_job
                                    .map(|(name, dur)| (name, SystemTime::now() - dur)),
                            })
                            .collect(),
                        ready_execs: status.ready_execs,
                        waiting_execs: status.waiting_execs,
                    });
                }
                Ok(ExecutorServerMessage::Done) => {
                    info!("Execution completed!");
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
        done.store(true, Ordering::Relaxed);
        status_poller.join().unwrap();
        Ok(())
    }
}

/// Process a file provided either by the client or by the server, calling the callback and writing
/// it to the `write_to` path. This will consume the iterator even if the callback is not present.
fn process_provided_file<I: IntoIterator<Item = Vec<u8>>>(
    file_callbacks: &mut HashMap<FileUuid, FileCallbacks>,
    uuid: FileUuid,
    success: bool,
    iterator: I,
) -> Result<(), Error> {
    if let Some(callback) = file_callbacks.get_mut(&uuid) {
        let limit = callback
            .get_content
            .as_ref()
            .map(|(limit, _)| *limit)
            .unwrap_or(0);
        let mut buffer: Vec<u8> = Vec::new();
        let mut file = match (&callback.write_to, success) {
            (Some((path, _)), true) => {
                std::fs::create_dir_all(path.parent().unwrap())?;
                Some(std::fs::File::create(path)?)
            }
            _ => None,
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
        drop(file);
        if let Some((path, executable)) = &callback.write_to {
            if *executable {
                let mut perm = std::fs::metadata(path)?.permissions();
                perm.set_mode(0o500);
                std::fs::set_permissions(path, perm)?;
            }
        }

        if let Some(get_content) = callback.get_content.take().map(|(_, f)| f) {
            get_content.call(buffer);
        }
    } else {
        iterator.into_iter().last();
    }
    Ok(())
}
