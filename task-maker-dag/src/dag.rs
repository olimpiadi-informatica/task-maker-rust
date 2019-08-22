use crate::file::*;
use crate::*;
use boxfnonce::BoxFnOnce;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use task_maker_store::*;

/// Configuration setting of an `ExecutionDAG`, some of the values set here will be inherited in the
/// configuration of the executions added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionDAGConfig {
    /// Keep the sandbox directory of each execution.
    pub keep_sandboxes: bool,
    /// Do not write any file inside the task directory. This works by inhibiting the calls to
    /// `write_file_to`, for this reason only the files added _after_ setting this value to `true`
    /// will be discarded.
    pub dry_run: bool,
}

/// A wrapper around a File provided by the client, this means that the client
/// knows the FileStoreKey and the path to that file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidedFile {
    /// The file handle.
    pub file: File,
    /// The key of the file for the lookup in the `FileStore`.
    pub key: FileStoreKey,
    /// Path to the file in the client.
    pub local_path: PathBuf,
}

/// Serializable part of the execution DAG: everything except the callbacks (which are not
/// serializable).
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGData {
    /// All the files provided by the client.
    pub provided_files: HashMap<FileUuid, ProvidedFile>,
    /// All the executions to run.
    pub executions: HashMap<ExecutionUuid, Execution>,
    /// The configuration of this DAG.
    pub config: ExecutionDAGConfig,
}

/// A computation DAG, this is not serializable because it contains the callbacks of the client.
#[derive(Debug)]
pub struct ExecutionDAG {
    /// Serializable part of the DAG with all the executions and files.
    pub data: ExecutionDAGData,
    /// Actual callbacks of the executions.
    pub execution_callbacks: HashMap<ExecutionUuid, ExecutionCallbacks>,
    /// Actual callbacks of the files.
    pub file_callbacks: HashMap<FileUuid, FileCallbacks>,
}

impl ExecutionDAG {
    /// Create an empty ExecutionDAG, without files and executions.
    pub fn new() -> ExecutionDAG {
        ExecutionDAG {
            data: ExecutionDAGData {
                provided_files: HashMap::new(),
                executions: HashMap::new(),
                config: ExecutionDAGConfig::new(),
            },
            execution_callbacks: HashMap::new(),
            file_callbacks: HashMap::new(),
        }
    }

    /// Provide a file for the computation.
    pub fn provide_file<P: Into<PathBuf>>(&mut self, file: File, path: P) -> Result<(), Error> {
        let path = path.into();
        self.data.provided_files.insert(
            file.uuid,
            ProvidedFile {
                file,
                key: FileStoreKey::from_file(&path)?,
                local_path: path,
            },
        );
        Ok(())
    }

    /// Add an execution to the DAG.
    pub fn add_execution(&mut self, mut execution: Execution) {
        execution.config = self.data.config.clone();
        self.data.executions.insert(execution.uuid, execution);
    }

    /// When `file` is ready it will be written to `path`. The file must be present in the dag
    /// before the evaluation starts.
    ///
    /// If the config `dry_run` is set to true the calls to this function are no-op.
    ///
    /// If the generation of the file fails (i.e. the `Execution` that produced that file was
    /// unsuccessful) the file is **not** written.
    pub fn write_file_to<F: Into<FileUuid>, P: Into<PathBuf>>(&mut self, file: F, path: P) {
        if !self.data.config.dry_run {
            self.file_callback(file.into()).write_to = Some(path.into());
        }
    }

    /// Call `callback` with the first `limit` bytes of the file when it's ready. The file must be
    /// present in the DAG before the evaluation starts.
    ///
    /// If the generation of the file fails (i.e. the `Execution` that produced that file was
    /// unsuccessful) the callback **is called** anyways with the content of the file, if any.
    pub fn get_file_content<G: Into<FileUuid>, F>(&mut self, file: G, limit: usize, callback: F)
    where
        F: (FnOnce(Vec<u8>) -> ()) + 'static,
    {
        self.file_callback(file.into()).get_content = Some((limit, BoxFnOnce::from(callback)));
    }

    /// Add a callback that will be called when the execution starts.
    pub fn on_execution_start<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(WorkerUuid) -> ()) + 'static,
    {
        self.execution_callback(execution)
            .on_start
            .push(BoxFnOnce::from(callback));
    }

    /// Add a callback that will be called when the execution ends.
    pub fn on_execution_done<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(ExecutionResult) -> ()) + 'static,
    {
        self.execution_callback(execution)
            .on_done
            .push(BoxFnOnce::from(callback));
    }

    /// Add a callback that will be called when the execution is skipped.
    pub fn on_execution_skip<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce() -> ()) + 'static,
    {
        self.execution_callback(execution)
            .on_skip
            .push(BoxFnOnce::from(callback));
    }

    /// Get a mutable reference to the config of this DAG.
    pub fn config_mut(&mut self) -> &mut ExecutionDAGConfig {
        &mut self.data.config
    }

    /// Makes sure that a callback item exists for that file and returns a &mut to it.
    fn file_callback<F: Into<FileUuid>>(&mut self, file: F) -> &mut FileCallbacks {
        self.file_callbacks.entry(file.into()).or_default()
    }

    /// Makes sure that a callback item exists for that execution and returns a &mut to it.
    fn execution_callback(&mut self, execution: &ExecutionUuid) -> &mut ExecutionCallbacks {
        self.execution_callbacks.entry(*execution).or_default()
    }
}

impl ExecutionDAGConfig {
    /// Make a new `ExecutionDAGConfig`.
    pub fn new() -> ExecutionDAGConfig {
        ExecutionDAGConfig {
            keep_sandboxes: false,
            dry_run: false,
        }
    }

    /// Whether to keep the sandbox directory of each execution.
    pub fn keep_sandboxes(&mut self, keep_sandboxes: bool) -> &mut Self {
        self.keep_sandboxes = keep_sandboxes;
        self
    }

    /// Whether to ignore all the subsequent calls to `write_file_to`.
    pub fn dry_run(&mut self, dry_run: bool) -> &mut Self {
        self.dry_run = dry_run;
        self
    }
}
