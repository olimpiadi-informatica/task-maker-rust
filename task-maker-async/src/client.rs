#![allow(dead_code)]
use std::{collections::HashMap, io::Read, sync::Arc};

use futures::future::try_join_all;
use tarpc::context;
use task_maker_dag::{ExecutionDAG as TMRExecutionDAG, FileUuid, ProvidedFile};
use tokio::{select, sync::Mutex, time::interval};
use tokio_util::sync::CancellationToken;

use anyhow::Error;

use crate::{
    dag::{ExecutionDAG, ExecutionDAGOptions},
    server::ServerClient,
    store::{
        DataIdentificationHash, FileSetFile, FileSetHandle, StoreClient, VariantIdentificationHash,
        LEASE_LENGTH,
    },
};

const BUF_SIZE: usize = 4 * 1024; // 4 KiB

struct FileSetHandleKeepalive {
    cancellation_token: Arc<CancellationToken>,
    store: StoreClient,
}

impl FileSetHandleKeepalive {
    fn new(store: &StoreClient) -> FileSetHandleKeepalive {
        FileSetHandleKeepalive {
            cancellation_token: Arc::new(CancellationToken::new()),
            store: store.clone(),
        }
    }

    fn register(&self, file_handle: FileSetHandle) {
        let store = self.store.clone();
        let token = self.cancellation_token.clone();
        tokio::spawn(async move {
            let mut timer = interval(LEASE_LENGTH);
            let timer = async {
                loop {
                    let _ = timer.tick().await;
                    {
                        store
                            .refresh_file_set_lease(context::current(), file_handle)
                            .await
                            .unwrap()
                            .unwrap();
                    }
                }
            };

            select! {
                _ = token.cancelled() => {}
                _ = timer => {}
            }
        });
    }
}

impl Drop for FileSetHandleKeepalive {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

struct FileIdentificationInfo {
    data_hash: DataIdentificationHash,
    variant_hash: VariantIdentificationHash,
    file: FileSetFile,
}

async fn ensure_input_available(
    file: &ProvidedFile,
    fileset_keepalive: &FileSetHandleKeepalive,
    file_uuid_to_hash: &Mutex<HashMap<FileUuid, FileIdentificationInfo>>,
    store: &StoreClient,
) -> Result<(), Error> {
    let mut hasher = blake3::Hasher::new();
    let file_info;
    match file {
        ProvidedFile::LocalFile {
            file,
            local_path: path,
            ..
        } => {
            file_info = file;
            let mut f = std::fs::File::open(path)?;
            let mut buf = [0; BUF_SIZE];
            loop {
                let n = f.read(&mut buf)?;
                hasher.update(&buf[..n]);
                if n == 0 {
                    break;
                }
            }
        }
        ProvidedFile::Content {
            file,
            content: data,
            ..
        } => {
            file_info = file;
            hasher.update(&data);
        }
    }

    let hash = *hasher.finalize().as_bytes();

    drop(hasher);

    file_uuid_to_hash.lock().await.insert(
        file_info.uuid,
        FileIdentificationInfo {
            data_hash: hash,
            variant_hash: hash,
            file: FileSetFile::MainFile,
        },
    );

    let mut handle = store
        .create_or_open_input_file(context::current(), hash)
        .await??;

    if handle.is_writable() {
        // File is not present, send it.
        let description_handle = store
            .open_file(context::current(), handle, FileSetFile::Metadata)
            .await??;
        store
            .append_chunk(
                context::current(),
                description_handle,
                file_info.description.as_bytes().to_vec(),
            )
            .await??;

        let file_handle = store
            .open_file(context::current(), handle, FileSetFile::MainFile)
            .await??;
        match file {
            ProvidedFile::LocalFile {
                local_path: path, ..
            } => {
                let mut f = std::fs::File::open(path)?;
                let mut buf = [0; BUF_SIZE];
                loop {
                    let n = f.read(&mut buf)?;
                    store
                        .append_chunk(context::current(), file_handle, buf[..n].to_vec())
                        .await??;
                    if n == 0 {
                        break;
                    }
                }
            }
            ProvidedFile::Content { content: data, .. } => {
                store
                    .append_chunk(context::current(), file_handle, data.clone())
                    .await??;
            }
        }

        handle = store
            .finalize_file_set(context::current(), handle)
            .await??;
    }

    assert!(!handle.is_writable());

    fileset_keepalive.register(handle);

    Ok(())
}

async fn evaluate_dag_async(
    dag: TMRExecutionDAG,
    store: &StoreClient,
    server: &ServerClient,
) -> Result<(), Error> {
    let fileset_keepalive = FileSetHandleKeepalive::new(store);
    let file_uuid_to_hash = Mutex::new(HashMap::new());

    try_join_all(
        dag.data.provided_files.values().map(|file| {
            ensure_input_available(file, &fileset_keepalive, &file_uuid_to_hash, store)
        }),
    )
    .await?;

    // TODO(veluca): create execution groups, setup urgent callbacks.
    server
        .evaluate(
            context::current(),
            ExecutionDAG {
                execution_groups: vec![],
            },
            ExecutionDAGOptions {
                keep_sandboxes: dag.data.config.keep_sandboxes,
                priority: dag.data.config.priority,
            },
        )
        .await??;

    // TODO(veluca): call non-urgent callbacks.

    Ok(())
}

/// Starts an async runtime and evaluates the given DAG in it.
pub fn evaluate_dag(
    dag: TMRExecutionDAG,
    store: &StoreClient,
    server: &ServerClient,
) -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(evaluate_dag_async(dag, store, server))
}
