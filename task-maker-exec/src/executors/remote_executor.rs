use std::sync::mpsc::channel;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use task_maker_cache::Cache;
use task_maker_store::FileStore;

use crate::executor::{Executor, ExecutorInMessage};
use crate::scheduler::ClientInfo;
use crate::{deserialize_from, ChannelServer, WorkerConn};

/// An executor that accepts remote connections from clients and workers.
pub struct RemoteExecutor {
    file_store: Arc<FileStore>,
}

/// Message sent only by remote clients and workers for sending its name.
#[derive(Debug, Serialize, Deserialize)]
pub enum RemoteEntityMessage {
    /// Tell the remote executor the name of the client or of the worker.
    Welcome {
        /// The name of the client or of the worker.
        name: String,
    },
}

impl RemoteExecutor {
    /// Make a new `RemoteExecutor`.
    pub fn new(file_store: Arc<FileStore>) -> Self {
        RemoteExecutor { file_store }
    }

    /// Start the executor binding the TCP sockets and waiting for clients and workers connections.
    pub fn start<S: Into<String>, S2: Into<String>>(
        self,
        bind_client_addr: S,
        bind_worker_addr: S2,
        cache: Cache,
    ) {
        let file_store = self.file_store;
        let bind_client_addr = bind_client_addr.into();
        let bind_worker_addr = bind_worker_addr.into();

        let (executor_tx, executor_rx) = channel();
        let executor = Executor::new(file_store, cache, executor_rx, true);

        let client_executor_tx = executor_tx.clone();
        let client_listener_thread = std::thread::Builder::new()
            .name("Client listener".to_string())
            .spawn(move || {
                let server =
                    ChannelServer::bind(&bind_client_addr).expect("Failed to bind client address");
                info!(
                    "Accepting client connections at tcp://{}",
                    server.local_addr().unwrap()
                );
                for (sender, receiver, addr) in server {
                    info!("Client connected from {}", addr);
                    let uuid = Uuid::new_v4();
                    let name = if let Ok(RemoteEntityMessage::Welcome { name }) =
                        deserialize_from(&receiver)
                    {
                        name
                    } else {
                        warn!(
                            "Client at {} has not sent the correct welcome message!",
                            addr
                        );
                        continue;
                    };
                    let client = ClientInfo { uuid, name };
                    client_executor_tx
                        .send(ExecutorInMessage::ClientConnected {
                            client,
                            sender,
                            receiver,
                        })
                        .expect("Executor is gone");
                }
            })
            .expect("Cannot spawn client listener thread");
        let worker_listener_thread = std::thread::Builder::new()
            .name("Worker listener".to_string())
            .spawn(move || {
                let server =
                    ChannelServer::bind(&bind_worker_addr).expect("Failed to bind worker address");
                info!(
                    "Accepting worker connections at tcp://{}",
                    server.local_addr().unwrap()
                );
                for (sender, receiver, addr) in server {
                    info!("Worker connected from {}", addr);
                    let uuid = Uuid::new_v4();
                    let name = if let Ok(RemoteEntityMessage::Welcome { name }) =
                        deserialize_from(&receiver)
                    {
                        name
                    } else {
                        warn!(
                            "Worker at {} has not sent the correct welcome message!",
                            addr
                        );
                        continue;
                    };
                    let worker = WorkerConn {
                        uuid,
                        name,
                        sender,
                        receiver,
                    };
                    executor_tx
                        .send(ExecutorInMessage::WorkerConnected { worker })
                        .expect("Executor is dead");
                }
            })
            .expect("Cannot spawn worker listener thread");

        executor.run().expect("Executor failed");

        client_listener_thread
            .join()
            .expect("Client listener failed");
        worker_listener_thread
            .join()
            .expect("Worker listener failed");
    }
}
