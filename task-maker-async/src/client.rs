#![allow(dead_code)]
use std::{
    collections::HashMap,
    io::{self, Read},
    sync::Arc,
    time::Duration,
};

use futures::future::try_join_all;
use tarpc::context;
use task_maker_dag::{
    CacheMode, ExecutionDAG as TMRExecutionDAG, ExecutionGroup as TMRExecutionGroup,
    ExecutionInput, ExecutionUuid, FileUuid, ProvidedFile,
};
use tokio::{select, sync::Mutex, time::interval};
use tokio_util::sync::CancellationToken;

use anyhow::{anyhow, Error};

use crate::{
    dag::{
        Execution, ExecutionConstraints, ExecutionDAG, ExecutionDAGOptions, ExecutionFileMode,
        ExecutionGroup, ExecutionInputFileInfo, ExecutionLimits, ExecutionPath,
        InputFilePermissions,
    },
    server::ServerClient,
    store::{
        DataIdentificationHash, ExecutionFile, FileSetFile, FileSetHandle, StoreClient,
        VariantIdentificationHash, LEASE_LENGTH,
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
    file_id: FileSetFile,
}

enum ProvidedFileChunkIterator {
    LocalFile(std::fs::File),
    Content(Option<Vec<u8>>),
}

impl ProvidedFileChunkIterator {
    fn new(file: &ProvidedFile) -> Result<ProvidedFileChunkIterator, io::Error> {
        match file {
            ProvidedFile::LocalFile {
                local_path: path, ..
            } => Ok(ProvidedFileChunkIterator::LocalFile(std::fs::File::open(
                path,
            )?)),
            ProvidedFile::Content { content: data, .. } => {
                Ok(ProvidedFileChunkIterator::Content(Some(data.clone())))
            }
        }
    }
}

impl Iterator for ProvidedFileChunkIterator {
    type Item = Result<Vec<u8>, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ProvidedFileChunkIterator::LocalFile(f) => {
                let mut buf = [0; BUF_SIZE];
                let n = f.read(&mut buf);
                match n {
                    Err(e) => Some(Err(e)),
                    Ok(0) => None,
                    Ok(n) => Some(Ok(buf[..n].to_vec())),
                }
            }
            ProvidedFileChunkIterator::Content(v) => v.take().map(Ok),
        }
    }
}

async fn ensure_input_available(
    file: &ProvidedFile,
    fileset_keepalive: &FileSetHandleKeepalive,
    file_uuid_to_hash: &Mutex<HashMap<FileUuid, FileIdentificationInfo>>,
    store: &StoreClient,
) -> Result<(), Error> {
    let mut hasher = blake3::Hasher::new();
    let file_info = match file {
        ProvidedFile::LocalFile { file, .. } => file,
        ProvidedFile::Content { file, .. } => file,
    };

    for data in ProvidedFileChunkIterator::new(file)? {
        hasher.update(&data?);
    }

    let hash = *hasher.finalize().as_bytes();

    drop(hasher);

    file_uuid_to_hash.lock().await.insert(
        file_info.uuid,
        FileIdentificationInfo {
            data_hash: hash,
            variant_hash: hash,
            file_id: FileSetFile::MainFile,
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
        for data in ProvidedFileChunkIterator::new(file)? {
            store
                .append_chunk(context::current(), file_handle, data?)
                .await??;
        }
        handle = store
            .finalize_file_set(context::current(), handle)
            .await??;
    }

    assert!(!handle.is_writable());

    fileset_keepalive.register(handle);

    Ok(())
}

struct StdoutStderrSize {
    stdout: Option<usize>,
    stderr: Option<usize>,
}

/// Returns None if the execution cannot be created yet or has been created already,
/// Some(converted_execution) otherwise.
fn prepare_execution_group(
    execution_group: &TMRExecutionGroup,
    dag: &TMRExecutionDAG,
    file_uuid_to_hash: &mut HashMap<FileUuid, FileIdentificationInfo>,
    execution_uuid_to_hash: &mut HashMap<ExecutionUuid, FileIdentificationInfo>,
    execution_uuid_to_stdout_stderr_size: &mut HashMap<ExecutionUuid, StdoutStderrSize>,
) -> Option<ExecutionGroup> {
    let is_done = execution_group
        .executions
        .iter()
        .map(|x| x.uuid)
        .any(|x| execution_uuid_to_hash.contains_key(&x));
    let can_process = execution_group
        .executions
        .iter()
        .flat_map(|x| {
            x.inputs
                .values()
                .map(|v| v.file)
                .chain(x.stdin.iter().cloned())
        })
        .all(|x| file_uuid_to_hash.contains_key(&x));
    if !can_process || is_done {
        return None;
    }

    let make_async_execution = |execution: &task_maker_dag::Execution| {
        let constraints = ExecutionConstraints {
            read_only: execution.limits.read_only,
            mount_tmpfs: execution.limits.mount_tmpfs,
            mount_proc: execution.limits.mount_proc,
            extra_readable_dirs: execution.limits.extra_readable_dirs.clone(),
        };
        let limits = ExecutionLimits {
            cpu_time: execution.limits.cpu_time.map(Duration::from_secs_f64),
            sys_time: execution.limits.sys_time.map(Duration::from_secs_f64),
            wall_time: execution.limits.wall_time.map(Duration::from_secs_f64),
            extra_time: Some(Duration::from_secs_f64(dag.data.config.extra_time)),
            memory: execution.limits.memory,
            nproc: execution.limits.nproc,
            fsize: execution.limits.fsize,
            nofile: execution.limits.nofile,
            memlock: execution.limits.memlock,
            stack: execution.limits.stack,
        };
        let mut files = vec![];

        let make_async_input = |file, executable| {
            let file_info = file_uuid_to_hash.get(file).unwrap();
            ExecutionInputFileInfo {
                permissions: if executable {
                    InputFilePermissions::Executable
                } else {
                    InputFilePermissions::Default
                },
                data_hash: file_info.data_hash,
                variant_hash: file_info.variant_hash,
                file_id: file_info.file_id.clone(),
            }
        };

        for (path, ExecutionInput { file, executable }) in execution.inputs.iter() {
            files.push((
                ExecutionPath::Path(path.clone()),
                ExecutionFileMode::Input(make_async_input(file, *executable)),
            ));
        }

        if let Some(file) = &execution.stdin {
            files.push((
                ExecutionPath::Stdin,
                ExecutionFileMode::Input(make_async_input(file, /*executable=*/ false)),
            ));
        }

        for path in execution.outputs.keys() {
            files.push((ExecutionPath::Path(path.clone()), ExecutionFileMode::Output));
        }

        if execution.stdout.is_some() {
            files.push((ExecutionPath::Stdout, ExecutionFileMode::Output));
        }
        if execution.stderr.is_some() {
            files.push((ExecutionPath::Stderr, ExecutionFileMode::Output));
        }

        // TODO(veluca): here we assume we only have FIFOs in std*_redirect_path.
        if let Some(fifo_path) = &execution.stdin_redirect_path {
            let name = fifo_path.file_name().unwrap().to_str().unwrap().to_string();
            files.push((ExecutionPath::Stdin, ExecutionFileMode::Fifo(name)));
        }
        if let Some(fifo_path) = &execution.stdout_redirect_path {
            let name = fifo_path.file_name().unwrap().to_str().unwrap().to_string();
            files.push((ExecutionPath::Stdout, ExecutionFileMode::Fifo(name)));
        }
        if let Some(fifo_path) = &execution.stderr_redirect_path {
            let name = fifo_path.file_name().unwrap().to_str().unwrap().to_string();
            files.push((ExecutionPath::Stderr, ExecutionFileMode::Fifo(name)));
        }

        for fifo in &execution_group.fifo {
            let name = fifo
                .sandbox_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            files.push((
                ExecutionPath::Path(fifo.sandbox_path()),
                ExecutionFileMode::Fifo(name),
            ));
        }

        // Ensure file order is deterministic across runs, as hash values will depend on it.
        files.sort();

        execution_uuid_to_stdout_stderr_size.insert(
            execution.uuid,
            StdoutStderrSize {
                stdout: execution.capture_stdout,
                stderr: execution.capture_stderr,
            },
        );

        Execution {
            name: execution.description.clone(),
            command: execution.command.clone(),
            args: execution.args.clone(),
            env: execution.env.clone().into_iter().collect(),
            copy_env: execution.copy_env.clone(),
            constraints,
            limits,
            files,
        }
    };

    let executions = execution_group
        .executions
        .iter()
        .map(make_async_execution)
        .collect();

    let priority = execution_group
        .executions
        .iter()
        .map(|x| x.priority)
        .max()
        .unwrap();

    let skip_cache_key = match &dag.data.config.cache_mode {
        CacheMode::Everything => None,
        CacheMode::Nothing => Some(uuid::Uuid::new_v4().to_string()),
        CacheMode::Except(to_not_cache) => {
            if execution_group
                .executions
                .iter()
                .flat_map(|x| x.tag.iter())
                .any(|x| to_not_cache.contains(x))
            {
                Some(uuid::Uuid::new_v4().to_string())
            } else {
                None
            }
        }
    };

    let ret = ExecutionGroup {
        description: execution_group.description.clone(),
        executions,
        skip_cache_key,
        priority,
    };

    let data_hash = ret.get_data_identification_hash();
    let variant_hash = ret.get_variant_identification_hash();

    for (async_execution, execution) in ret.executions.iter().zip(execution_group.executions.iter())
    {
        execution_uuid_to_hash.insert(
            execution.uuid,
            FileIdentificationInfo {
                data_hash,
                variant_hash,
                file_id: FileSetFile::AuxiliaryFile(
                    async_execution.name.clone(),
                    ExecutionFile::Outcome,
                ),
            },
        );
        for (path, file_info) in execution.outputs.iter() {
            file_uuid_to_hash.insert(
                file_info.uuid,
                FileIdentificationInfo {
                    data_hash,
                    variant_hash,
                    file_id: FileSetFile::AuxiliaryFile(
                        async_execution.name.clone(),
                        ExecutionFile::File(path.clone()),
                    ),
                },
            );
        }

        if let Some(file) = &execution.stdout {
            file_uuid_to_hash.insert(
                file.uuid,
                FileIdentificationInfo {
                    data_hash,
                    variant_hash,
                    file_id: FileSetFile::AuxiliaryFile(
                        async_execution.name.clone(),
                        ExecutionFile::Stdout,
                    ),
                },
            );
        }
        if let Some(file) = &execution.stderr {
            file_uuid_to_hash.insert(
                file.uuid,
                FileIdentificationInfo {
                    data_hash,
                    variant_hash,
                    file_id: FileSetFile::AuxiliaryFile(
                        async_execution.name.clone(),
                        ExecutionFile::Stderr,
                    ),
                },
            );
        }
    }

    Some(ret)
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

    let mut execution_groups = vec![];
    let mut execution_uuid_to_hash = HashMap::new();
    let mut execution_uuid_to_stdout_stderr_size = HashMap::new();

    // Prepare the execution groups for the async DAG. TODO(veluca): be less quadratic.
    loop {
        let num_groups = execution_groups.len();
        if num_groups == dag.data.execution_groups.len() {
            break;
        }

        let mut locked_file_info = file_uuid_to_hash.lock().await;

        execution_groups.extend(
            dag.data
                .execution_groups
                .values()
                .flat_map(|execution_group| {
                    prepare_execution_group(
                        execution_group,
                        &dag,
                        &mut locked_file_info,
                        &mut execution_uuid_to_hash,
                        &mut execution_uuid_to_stdout_stderr_size,
                    )
                    .into_iter()
                }),
        );

        if num_groups == execution_groups.len() {
            return Err(anyhow!("Input DAG is not a DAG"));
        }
    }

    // TODO(veluca): setup urgent callbacks.
    server
        .evaluate(
            context::current(),
            ExecutionDAG { execution_groups },
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
