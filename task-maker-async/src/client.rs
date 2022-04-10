#![allow(dead_code)]
use std::{
    collections::HashMap, os::unix::prelude::PermissionsExt, path::Path, sync::Arc, time::Duration,
};

use futures::future::{try_join3, try_join_all};
use tarpc::context;
use task_maker_dag::{
    CacheMode, ExecutionCallbacks, ExecutionDAG as TMRExecutionDAG,
    ExecutionGroup as TMRExecutionGroup, ExecutionInput, ExecutionResult, ExecutionUuid,
    FileCallbacks, FileUuid, ProvidedFile, WorkerUuid,
};
use tokio::{
    fs::{create_dir_all, File},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom},
    select,
    sync::Mutex,
    time::interval,
};
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
        ComputationOutcome, DataIdentificationHash, ExecutionFile, FileHandle, FileReadingOutcome,
        FileSetFile, FileSetHandle, StoreClient, VariantIdentificationHash, LEASE_LENGTH,
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

/// An iterator-like utility for reading a [`ProvidedFile`] in chunks.
///
/// The vast majority of the times the `LocalFile` variant is used, therefore we avoid boxing the
/// buffer.
#[allow(clippy::large_enum_variant)]
enum ProvidedFileChunkIterator<'a> {
    LocalFile { buffer: [u8; BUF_SIZE], file: File },
    Content { content: &'a [u8], consumed: bool },
}

impl<'a> ProvidedFileChunkIterator<'a> {
    async fn new(provided_file: &'a ProvidedFile) -> Result<ProvidedFileChunkIterator<'a>, Error> {
        match provided_file {
            ProvidedFile::LocalFile { local_path, .. } => Ok(Self::LocalFile {
                file: File::open(local_path).await?,
                buffer: [0; BUF_SIZE],
            }),
            ProvidedFile::Content { content, .. } => Ok(Self::Content {
                content,
                consumed: false,
            }),
        }
    }

    async fn next(&mut self) -> Result<Option<&[u8]>, Error> {
        match self {
            ProvidedFileChunkIterator::LocalFile { buffer, file } => {
                let size = file.read(buffer).await?;
                if size == 0 {
                    Ok(None)
                } else {
                    Ok(Some(&buffer[..size]))
                }
            }
            ProvidedFileChunkIterator::Content { content, consumed } => {
                if *consumed {
                    Ok(None)
                } else {
                    *consumed = true;
                    Ok(Some(*content))
                }
            }
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
    let mut reader = ProvidedFileChunkIterator::new(file).await?;
    while let Some(chunk) = reader.next().await? {
        hasher.update(chunk);
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

        let mut reader = ProvidedFileChunkIterator::new(file).await?;
        while let Some(chunk) = reader.next().await? {
            store
                .append_chunk(context::current(), file_handle, chunk.into())
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

async fn read_file_to_memory(
    store: &StoreClient,
    file: &FileHandle,
    size_limit: Option<usize>,
) -> Result<Vec<u8>, Error> {
    let mut result = vec![];
    loop {
        if let Some(limit) = size_limit {
            if result.len() >= limit {
                break;
            }
        }
        let chunk = store
            .read_chunk(context::current(), *file, result.len())
            .await??;
        match chunk {
            FileReadingOutcome::Dropped => {
                result.clear();
            }
            FileReadingOutcome::EndOfFile => {
                break;
            }
            FileReadingOutcome::Data(chunk) => {
                result.extend(chunk);
            }
        };
    }
    Ok(result)
}

async fn write_file_to_disk(
    store: &StoreClient,
    file: &FileHandle,
    destination: &Path,
    make_executable: bool,
) -> Result<(), Error> {
    create_dir_all(destination.parent().unwrap()).await?;
    let mut destination = File::create(destination).await?;
    if make_executable {
        destination
            .set_permissions(PermissionsExt::from_mode(0o755))
            .await?;
    }
    loop {
        let chunk = store
            .read_chunk(
                context::current(),
                *file,
                destination.stream_position().await? as usize,
            )
            .await??;
        match chunk {
            FileReadingOutcome::Dropped => {
                destination.set_len(0).await?;
                destination.seek(SeekFrom::Start(0)).await?;
            }
            FileReadingOutcome::EndOfFile => {
                break;
            }
            FileReadingOutcome::Data(chunk) => {
                destination.write_all(&chunk).await?;
            }
        };
    }
    Ok(())
}

async fn get_execution_result(
    store: &StoreClient,
    file_set_handle: &FileSetHandle,
    execution_name: String,
    stdout_stderr_size: StdoutStderrSize,
) -> Result<ExecutionResult, Error> {
    let result = store
        .open_file(
            context::current(),
            *file_set_handle,
            FileSetFile::AuxiliaryFile(execution_name.clone(), ExecutionFile::Outcome),
        )
        .await??;
    let mut result: ExecutionResult =
        bincode::deserialize(&read_file_to_memory(store, &result, None).await?)?;

    if let Some(stdout_size) = stdout_stderr_size.stdout {
        let out = store
            .open_file(
                context::current(),
                *file_set_handle,
                FileSetFile::AuxiliaryFile(execution_name.clone(), ExecutionFile::Stdout),
            )
            .await??;
        result.stdout = Some(read_file_to_memory(store, &out, Some(stdout_size)).await?);
    }

    if let Some(stderr_size) = stdout_stderr_size.stderr {
        let out = store
            .open_file(
                context::current(),
                *file_set_handle,
                FileSetFile::AuxiliaryFile(execution_name.clone(), ExecutionFile::Stderr),
            )
            .await??;
        result.stderr = Some(read_file_to_memory(store, &out, Some(stderr_size)).await?);
    }

    Ok(result)
}

async fn execution_callback(
    store: &StoreClient,
    execution: FileIdentificationInfo,
    callback: ExecutionCallbacks,
    stdout_stderr_size: StdoutStderrSize,
) -> Result<(), Error> {
    let file_set_handle = store
        .open_computation(
            context::current(),
            execution.data_hash,
            execution.variant_hash,
        )
        .await??;

    let worker_uuid = WorkerUuid::new_v4(); // TODO(veluca): this is not a true worker id.

    for cb in callback.on_start.into_iter() {
        cb(worker_uuid)?;
    }

    store
        .wait_until_finalized(context::current(), file_set_handle)
        .await??;

    let status = store
        .open_file(context::current(), file_set_handle, FileSetFile::MainFile)
        .await??;

    let status: ComputationOutcome =
        bincode::deserialize(&read_file_to_memory(store, &status, None).await?)?;

    match status {
        ComputationOutcome::Skipped => {
            for cb in callback.on_skip.into_iter() {
                cb()?;
            }
        }
        ComputationOutcome::Executed => {
            let name = if let FileSetFile::AuxiliaryFile(name, _) = execution.file_id {
                name
            } else {
                panic!("Invalid execution FileIdentificationInfo");
            };

            let result =
                get_execution_result(store, &file_set_handle, name, stdout_stderr_size).await?;
            for cb in callback.on_done.into_iter() {
                cb(result.clone())?;
            }
        }
    };

    Ok(())
}

async fn file_callback(
    store: &StoreClient,
    file: FileIdentificationInfo,
    callback: FileCallbacks,
) -> Result<(), Error> {
    let file_set_handle = store
        .open_computation(context::current(), file.data_hash, file.variant_hash)
        .await??;

    store
        .wait_until_finalized(context::current(), file_set_handle)
        .await??;

    let status = store
        .open_file(context::current(), file_set_handle, FileSetFile::MainFile)
        .await??;

    let status: ComputationOutcome =
        bincode::deserialize(&read_file_to_memory(store, &status, None).await?)?;

    if status != ComputationOutcome::Executed {
        // Nothing to do.
        return Ok(());
    }

    let execution_name = if let FileSetFile::AuxiliaryFile(name, _) = &file.file_id {
        name
    } else {
        panic!("Invalid execution FileIdentificationInfo");
    };

    let result = get_execution_result(
        store,
        &file_set_handle,
        execution_name.clone(),
        StdoutStderrSize {
            stdout: None,
            stderr: None,
        },
    )
    .await?;

    if let Some(write_to) = callback.write_to {
        let file = store
            .open_file(context::current(), file_set_handle, file.file_id.clone())
            .await??;
        if write_to.allow_failure || result.status.is_success() {
            write_file_to_disk(store, &file, &write_to.dest, write_to.executable).await?;
        }
    }

    if result.status.is_success() {
        if let Some((size, cb)) = callback.get_content {
            let file = store
                .open_file(context::current(), file_set_handle, file.file_id)
                .await??;
            let file = read_file_to_memory(store, &file, Some(size)).await?;
            cb(file)?;
        }
    }

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

    let mut execution_groups = vec![];
    let mut execution_uuid_to_hash = HashMap::new();
    let mut execution_uuid_to_stdout_stderr_size = HashMap::new();

    let mut file_uuid_to_hash = file_uuid_to_hash.lock().await;

    // Prepare the execution groups for the async DAG. TODO(veluca): be less quadratic.
    loop {
        let num_groups = execution_groups.len();
        if num_groups == dag.data.execution_groups.len() {
            break;
        }

        execution_groups.extend(
            dag.data
                .execution_groups
                .values()
                .flat_map(|execution_group| {
                    prepare_execution_group(
                        execution_group,
                        &dag,
                        &mut file_uuid_to_hash,
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

    let mut callbacks = dag.callbacks.unwrap();

    let wait_execution_callbacks = try_join_all(callbacks.execution_callbacks.into_iter().map(
        |(uuid, callback)| {
            execution_callback(
                store,
                execution_uuid_to_hash.remove(&uuid).unwrap(),
                callback,
                execution_uuid_to_stdout_stderr_size.remove(&uuid).unwrap(),
            )
        },
    ));

    let wait_urgent_file_callbacks = try_join_all(callbacks.urgent_files.into_iter().map(|uuid| {
        file_callback(
            store,
            file_uuid_to_hash.remove(&uuid).unwrap(),
            callbacks.file_callbacks.remove(&uuid).unwrap(),
        )
    }));

    let wait_eval_done = async {
        server
            .evaluate(
                context::current(),
                ExecutionDAG { execution_groups },
                ExecutionDAGOptions {
                    keep_sandboxes: dag.data.config.keep_sandboxes,
                    priority: dag.data.config.priority,
                },
            )
            .await
            // Flatten to Result<(), Error>
            .map_err(anyhow::Error::new)
            .and_then(|x| Ok(x?))
    };

    try_join3(
        wait_eval_done,
        wait_urgent_file_callbacks,
        wait_execution_callbacks,
    )
    .await?;

    // Run non-urgent file callbacks.
    try_join_all(
        callbacks
            .file_callbacks
            .into_iter()
            .map(|(uuid, callback)| {
                file_callback(store, file_uuid_to_hash.remove(&uuid).unwrap(), callback)
            }),
    )
    .await?;

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
