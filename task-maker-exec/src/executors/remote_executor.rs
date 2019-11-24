use crate::*;
use std::sync::Arc;
use task_maker_cache::Cache;

/// An executor that accepts remote connections from clients and workers.
pub struct RemoteExecutor {
    file_store: Arc<FileStore>,
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
        let file_store = self.file_store.clone();
        let bind_client_addr = bind_client_addr.into();
        let bind_worker_addr = bind_worker_addr.into();
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
                    // TODO handle client connection
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
                    // TODO handle worker connection
                }
            })
            .expect("Cannot spawn worker listener thread");
        client_listener_thread
            .join()
            .expect("Client listener failed");
        worker_listener_thread
            .join()
            .expect("Worker listener failed");
    }
}
