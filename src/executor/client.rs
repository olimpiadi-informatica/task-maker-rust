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
        serialize_into(&ExecutorClientMessage::Evaluate(dag.data), &sender)?;
        serialize_into(&ExecutorClientMessage::Status, &sender)?;
        info!("{} callbacks", dag.execution_callbacks.len());
        for x in dag.execution_callbacks.iter() {
            x.1.on_start.as_ref().unwrap()(*x.0);
        }
        loop {
            match deserialize_from::<ExecutorServerMessage>(&receiver) {
                Ok(ExecutorServerMessage::AskFile(uuid)) => {
                    info!("Server is asking for {}", uuid);
                    serialize_into(&ExecutorClientMessage::ProvideFile(uuid), &sender)?;
                }
                Ok(ExecutorServerMessage::NotifyStart(uuid, worker)) => {
                    info!("Execution {} started on {}", uuid, worker);
                    // TODO call callback
                }
                Ok(ExecutorServerMessage::NotifyDone(uuid, result)) => {
                    info!("Execution {} completed with {}", uuid, result);
                    // TODO call callback
                }
                Ok(ExecutorServerMessage::NotifySkip(uuid)) => {
                    info!("Execution {} skipped", uuid);
                    // TODO call callback
                }
                Ok(ExecutorServerMessage::Error(error)) => {
                    info!("Error occurred: {}", error);
                    // TODO abort
                }
                Ok(ExecutorServerMessage::Status(status)) => {
                    info!("Server status: {}", status);
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
