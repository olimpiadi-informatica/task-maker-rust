use crate::execution::*;
use crate::executor::*;
use failure::Error;
use std::sync::mpsc::{Receiver, Sender};

pub struct ExecutorClient;

impl ExecutorClient {
    pub fn evaluate(
        dag: ExecutionDAG,
        sender: Sender<String>,
        receiver: Receiver<String>,
    ) -> Result<(), Error> {
        info!("ExecutorClient started");
        let dag_callbacks = ExecutionDAGCallbacks {
            executions: dag.execution_callbacks.keys().map(|k| k.clone()).collect(),
            files: dag.file_callbacks.keys().map(|k| k.clone()).collect(),
        };
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
                    serialize_into(&ExecutorClientMessage::ProvideFile(uuid), &sender)?;
                }
                Ok(ExecutorServerMessage::ProvideFile(uuid)) => {
                    info!("Server sent the file {}", uuid);
                    if let Some(callback) = dag.file_callbacks.get(&uuid) {
                        if let Some(write_to) = callback.write_to.as_ref() {
                            info!("Writing {} to {}", uuid, write_to);
                        }
                        if let Some((_limit, get_content)) = callback.get_content.as_ref() {
                            get_content(vec![1, 2, 3, 42]);
                        }
                    }
                }
                Ok(ExecutorServerMessage::NotifyStart(uuid, worker)) => {
                    info!("Execution {} started on {}", uuid, worker);
                    if let Some(callbacks) = dag.execution_callbacks.get(&uuid) {
                        if let Some(callback) = &callbacks.on_start {
                            callback(worker);
                        }
                    }
                }
                Ok(ExecutorServerMessage::NotifyDone(uuid, result)) => {
                    info!("Execution {} completed with {:?}", uuid, result);
                    if let Some(callbacks) = dag.execution_callbacks.get(&uuid) {
                        if let Some(callback) = &callbacks.on_done {
                            callback(result);
                        }
                    }
                }
                Ok(ExecutorServerMessage::NotifySkip(uuid)) => {
                    info!("Execution {} skipped", uuid);
                    if let Some(callbacks) = dag.execution_callbacks.get(&uuid) {
                        if let Some(callback) = &callbacks.on_skip {
                            callback();
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
                    info!("Connection error: {}", cause);
                    if cause == "receiving on a closed channel" {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
