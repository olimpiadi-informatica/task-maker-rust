use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

use anyhow::{anyhow, Context, Error};
use ductile::{ChannelSender, ChannelServer};
use serde::{Deserialize, Serialize};
use task_maker_cache::Cache;
use task_maker_store::FileStore;
use uuid::Uuid;

use crate::executor::{Executor, ExecutorInMessage};
use crate::scheduler::ClientInfo;
use crate::{derive_key_from_password, WorkerConn};

/// Version of task-maker
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An executor that accepts remote connections from clients and workers.
pub struct RemoteExecutor {
    file_store: Arc<FileStore>,
}

/// Message sent only by remote clients and workers for connecting to the server.
#[derive(Debug, Serialize, Deserialize)]
pub enum RemoteEntityMessage {
    /// Tell the remote executor the name of the client or of the worker.
    Welcome {
        /// The name of the client or of the worker.
        name: String,
        /// The required version of task-maker.
        version: String,
    },
}

/// Message sent only by the server in response of a `RemoteEntityMessage`.
#[derive(Debug, Serialize, Deserialize)]
pub enum RemoteEntityMessageResponse {
    /// The server accepted the connection of the client, the communication can continue.
    Accepted,
    /// The server rejected the connection of the client, the channel will be closed.
    Rejected(String),
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
        client_password: Option<String>,
        worker_password: Option<String>,
        cache: Cache,
    ) -> Result<(), Error> {
        let file_store = self.file_store;
        let bind_client_addr = bind_client_addr.into();
        let bind_worker_addr = bind_worker_addr.into();

        let (executor_tx, executor_rx) = channel();
        let executor = Executor::new(file_store, cache, executor_rx, true);

        let client_executor_tx = executor_tx.clone();
        let client_listener_thread = std::thread::Builder::new()
            .name("Client listener".to_string())
            .spawn(move || {
                Self::client_listener(client_password, bind_client_addr, client_executor_tx)
            })
            .context("Cannot spawn client listener thread")?;
        let worker_listener_thread = std::thread::Builder::new()
            .name("Worker listener".to_string())
            .spawn(move || Self::worker_listener(worker_password, bind_worker_addr, executor_tx))
            .context("Cannot spawn worker listener thread")?;

        executor.run()?;

        client_listener_thread
            .join()
            .map_err(|e| anyhow!("Client listener panicked: {:?}", e))?
            .context("Client listener failed")?;
        worker_listener_thread
            .join()
            .map_err(|e| anyhow!("Worker listener panicked: {:?}", e))?
            .context("Worker listener failed")?;
        Ok(())
    }

    fn client_listener(
        client_password: Option<String>,
        bind_client_addr: String,
        client_executor_tx: Sender<ExecutorInMessage>,
    ) -> Result<(), Error> {
        let server = if let Some(path) = bind_client_addr.strip_prefix("unix://") {
            ChannelServer::bind_unix(path)
                .with_context(|| format!("Failed to bind client unix socket at {path}"))?
        } else {
            match client_password {
                Some(password) => {
                    let key = derive_key_from_password(password);
                    ChannelServer::bind_with_enc(&bind_client_addr, key)
                        .context("Failed to bind client address")?
                }
                None => ChannelServer::bind(&bind_client_addr)
                    .context("Failed to bind client address")?,
            }
        };

        let local_addr = server
            .local_addr()
            .context("Failed to get client address")?;
        info!(
            "Accepting client connections at {}",
            if let Some(addr) = local_addr {
                format!("tcp://{addr}")
            } else {
                bind_client_addr
            }
        );
        for (sender, receiver, addr) in server {
            let addr = addr
                .map(|s| s.to_string())
                .unwrap_or_else(|| "(local)".into());
            info!("Client connected from {addr}");
            let uuid = Uuid::new_v4();
            let name = if let Ok(RemoteEntityMessage::Welcome { name, version }) = receiver.recv() {
                if !validate_welcome(&addr, &name, version, &sender, "Client") {
                    continue;
                }
                name
            } else {
                warn!("Client at {addr} has not sent the correct welcome message!");
                continue;
            };
            let client = ClientInfo { uuid, name };
            client_executor_tx
                .send(ExecutorInMessage::ClientConnected {
                    client,
                    sender: sender.change_type(),
                    receiver: receiver.change_type(),
                })
                .map_err(|e| anyhow!("Executor is gone: {:?}", e))?;
        }
        Ok(())
    }

    fn worker_listener(
        worker_password: Option<String>,
        bind_worker_addr: String,
        executor_tx: Sender<ExecutorInMessage>,
    ) -> Result<(), Error> {
        let server = if let Some(path) = bind_worker_addr.strip_prefix("unix://") {
            ChannelServer::bind_unix(path)
                .with_context(|| format!("Failed to bind worker unix socket at {path}"))?
        } else {
            match worker_password {
                Some(password) => {
                    let key = derive_key_from_password(password);
                    ChannelServer::bind_with_enc(&bind_worker_addr, key)
                        .context("Failed to bind worker address")?
                }
                None => ChannelServer::bind(&bind_worker_addr)
                    .context("Failed to bind worker address")?,
            }
        };
        let local_addr = server
            .local_addr()
            .context("Failed to get worker address")?;
        info!(
            "Accepting worker connections at {}",
            if let Some(addr) = local_addr {
                format!("tcp://{addr}")
            } else {
                bind_worker_addr
            }
        );
        for (sender, receiver, addr) in server {
            let addr = addr
                .map(|s| s.to_string())
                .unwrap_or_else(|| "(local)".into());
            info!("Worker connected from {addr}");
            let uuid = Uuid::new_v4();
            let name = if let Ok(RemoteEntityMessage::Welcome { name, version }) = receiver.recv() {
                if !validate_welcome(&addr, &name, version, &sender, "Worker") {
                    continue;
                }
                name
            } else {
                warn!("Worker at {addr} has not sent the correct welcome message!");
                continue;
            };
            let worker = WorkerConn {
                uuid,
                name,
                sender: sender.change_type(),
                receiver: receiver.change_type(),
            };
            executor_tx
                .send(ExecutorInMessage::WorkerConnected { worker })
                .map_err(|e| anyhow!("Executor is gone: {:?}", e))?;
        }
        Ok(())
    }
}

fn validate_welcome(
    addr: &str,
    name: &str,
    version: String,
    sender: &ChannelSender<RemoteEntityMessageResponse>,
    client: &str,
) -> bool {
    if version != VERSION {
        warn!(
            "{client} '{name}' from {addr} connected with version {version}, server has {VERSION}"
        );
        let _ = sender.send(RemoteEntityMessageResponse::Rejected(format!(
            "Wrong task-maker version, you have {version}, server has {VERSION}"
        )));
        false
    } else {
        let _ = sender.send(RemoteEntityMessageResponse::Accepted);
        true
    }
}
