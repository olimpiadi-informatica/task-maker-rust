use crate::execution::execution::*;
use crate::execution::file::*;
use crate::executor::*;
use crate::store::*;
use failure::Fail;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// A wrapper around a File provided by the client, this means that the client
/// knows the FileStoreKey and the path to that file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidedFile {
    /// The file handle
    pub file: File,
    /// The key in the FileStore
    pub key: FileStoreKey,
    /// Path to the file in the client
    pub local_path: PathBuf,
}

/// Serializable part of the execution DAG, this is sent to the server.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGData {
    /// List of the files provided by the client
    pub provided_files: HashMap<FileUuid, ProvidedFile>,
    /// List of the executions to run
    pub executions: HashMap<ExecutionUuid, Execution>,
}

/// List of the "interesting" files and executions, only the callbacks listed
/// here will be called by the server
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGCallbacks {
    /// Set of the handles of the executions that have at least a callback
    /// bound
    pub executions: HashSet<ExecutionUuid>,
    /// Set of the handles of the files that have at least a callback bound
    pub files: HashSet<FileUuid>,
}

/// A computation DAG, this is not serializable because it contains the
/// callbacks of the client
#[derive(Debug)]
pub struct ExecutionDAG {
    /// Serializable part of the DAG with all the exections and files
    pub data: ExecutionDAGData,
    /// Actual callbacks of the executions
    pub execution_callbacks: HashMap<ExecutionUuid, ExecutionCallbacks>,
    /// Actual callbacks of the files
    pub file_callbacks: HashMap<FileUuid, FileCallbacks>,
}

#[derive(Debug, Fail)]
pub enum DAGError {
    #[fail(display = "missing file {} ({})", description, uuid)]
    MissingFile { uuid: FileUuid, description: String },
    #[fail(display = "missing execution {}", uuid)]
    MissingExecution { uuid: FileUuid },
    #[fail(
        display = "detected dependency cycle, '{}' is in the cycle",
        description
    )]
    CycleDetected { description: String },
    #[fail(display = "duplicate execution UUID {}", uuid)]
    DuplicateExecutionUUID { uuid: ExecutionUuid },
    #[fail(display = "duplicate file UUID {}", uuid)]
    DuplicateFileUUID { uuid: FileUuid },
}

/// Value returned by [ExecutionDAG](struct.ExecutionDAG.html).[add_execution](
/// struct.ExecutionDAG.html#method.add_execution) to make a
/// Builder for setting the callbacks
pub struct AddExecutionWrapper<'a> {
    uuid: ExecutionUuid,
    dag: &'a mut ExecutionDAG,
}

impl ExecutionDAG {
    /// Create an empty ExecutionDAG
    pub fn new() -> ExecutionDAG {
        ExecutionDAG {
            data: ExecutionDAGData {
                provided_files: HashMap::new(),
                executions: HashMap::new(),
            },
            execution_callbacks: HashMap::new(),
            file_callbacks: HashMap::new(),
        }
    }

    /// Provide a file for the computation
    ///
    /// Will panic if the file doesn't exists or it's not readable
    pub fn provide_file(&mut self, file: File, path: &Path) {
        self.data.provided_files.insert(
            file.uuid.clone(),
            ProvidedFile {
                file,
                key: FileStoreKey::from_file(path).expect("Cannot compute FileStoreKey"),
                local_path: path.to_owned(),
            },
        );
    }

    /// Add an execution to the DAG and returns a Builder for adding the
    /// callbacks
    pub fn add_execution(&mut self, execution: Execution) -> AddExecutionWrapper {
        let uuid = execution.uuid.clone();
        self.data
            .executions
            .insert(execution.uuid.clone(), execution);
        AddExecutionWrapper {
            uuid: uuid,
            dag: self,
        }
    }
}

impl<'a> AddExecutionWrapper<'a> {
    /// Set that callback that will be called when the execution starts
    pub fn on_start<F>(mut self, callback: F) -> AddExecutionWrapper<'a>
    where
        F: (Fn(WorkerUuid) -> ()) + 'static,
    {
        self.ensure_execution_callback().on_start = Some(Box::new(callback));
        self
    }

    /// Set that callback that will be called when the execution ends
    pub fn on_done<F>(mut self, callback: F) -> AddExecutionWrapper<'a>
    where
        F: (Fn(WorkerResult) -> ()) + 'static,
    {
        self.ensure_execution_callback().on_done = Some(Box::new(callback));
        self
    }

    /// Set that callback that will be called when the execution is skipped
    pub fn on_skip<F>(mut self, callback: F) -> AddExecutionWrapper<'a>
    where
        F: (Fn() -> ()) + 'static,
    {
        self.ensure_execution_callback().on_skip = Some(Box::new(callback));
        self
    }

    /// Write the standard output of the execution to `path`
    pub fn write_stdout_to(mut self, path: &Path) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stdout().uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    /// Write the standard error of the execution to `path`
    pub fn write_stderr_to(mut self, path: &Path) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stderr().uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    /// Write the output of the execution at `output` to `path`
    pub fn write_output_to(mut self, output: &Path, path: &Path) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().output(output).uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    /// Set that callback that will be called with the first `limit` bytes of
    /// the standard output
    pub fn get_stdout_content<F>(mut self, limit: usize, callback: F) -> AddExecutionWrapper<'a>
    where
        F: (Fn(Vec<u8>) -> ()) + 'static,
    {
        let uuid = self.get_execution().stdout().uuid.clone();
        self.bind_get_content(limit, Box::new(callback), uuid);
        self
    }

    /// Set that callback that will be called with the first `limit` bytes of
    /// the standard error
    pub fn get_stderr_content<F>(mut self, limit: usize, callback: F) -> AddExecutionWrapper<'a>
    where
        F: (Fn(Vec<u8>) -> ()) + 'static,
    {
        let uuid = self.get_execution().stderr().uuid.clone();
        self.bind_get_content(limit, Box::new(callback), uuid);
        self
    }

    /// Set that callback that will be called with the first `limit` bytes of
    /// the file at `output`
    pub fn get_output_content<F>(
        mut self,
        output: &Path,
        limit: usize,
        callback: F,
    ) -> AddExecutionWrapper<'a>
    where
        F: (Fn(Vec<u8>) -> ()) + 'static,
    {
        let uuid = self.get_execution().output(output).uuid.clone();
        self.bind_get_content(limit, Box::new(callback), uuid);
        self
    }

    /// Ensures the callback is present and store the path of where to store
    /// the file
    fn write_file_to(&mut self, path: &Path, uuid: FileUuid) {
        self.ensure_file_callback(&uuid);
        self.dag.file_callbacks.get_mut(&uuid).unwrap().write_to = Some(path.to_owned());
    }

    /// Ensures the callback is present and store the callback
    fn bind_get_content(
        &mut self,
        limit: usize,
        callback: Box<GetContentCallback>,
        uuid: FileUuid,
    ) {
        self.ensure_file_callback(&uuid);
        self.dag.file_callbacks.get_mut(&uuid).unwrap().get_content = Some((limit, callback));
    }

    fn ensure_file_callback(&mut self, uuid: &FileUuid) {
        if !self.dag.file_callbacks.contains_key(&uuid) {
            self.dag
                .file_callbacks
                .insert(uuid.clone(), FileCallbacks::default());
        }
    }

    fn ensure_execution_callback(&mut self) -> &mut ExecutionCallbacks {
        if !self.dag.execution_callbacks.contains_key(&self.uuid) {
            self.dag
                .execution_callbacks
                .insert(self.uuid.clone(), ExecutionCallbacks::default());
        }
        self.dag.execution_callbacks.get_mut(&self.uuid).unwrap()
    }

    fn get_execution(&mut self) -> &mut Execution {
        self.dag.data.executions.get_mut(&self.uuid).unwrap()
    }
}

/// Validate the DAG checking if all the required pieces are present and they
/// actually make a DAG. It's checked that no duplicated UUID are present, no
/// files are missing, all the executions are reachable and no cycles are
/// present
pub fn check_dag(
    dag: &ExecutionDAGData,
    callbacks: &ExecutionDAGCallbacks,
) -> Result<(), DAGError> {
    let mut dependencies: HashMap<FileUuid, Vec<ExecutionUuid>> = HashMap::new();
    let mut num_dependencies: HashMap<ExecutionUuid, usize> = HashMap::new();
    let mut known_files: HashSet<FileUuid> = HashSet::new();
    let mut ready_execs: VecDeque<ExecutionUuid> = VecDeque::new();
    let mut ready_files: VecDeque<FileUuid> = VecDeque::new();

    let mut add_dependency = |file: FileUuid, exec: ExecutionUuid| {
        if !dependencies.contains_key(&file) {
            dependencies.insert(file, vec![exec]);
        } else {
            dependencies.get_mut(&file).unwrap().push(exec);
        }
    };

    // add the exectutions and check for duplicated UUIDs
    for exec_uuid in dag.executions.keys() {
        let exec = dag.executions.get(exec_uuid).expect("No such exec");
        let deps = exec.dependencies();
        let count = deps.len();
        for dep in deps.into_iter() {
            add_dependency(dep, exec_uuid.clone());
        }
        for out in exec.outputs().into_iter() {
            if !known_files.insert(out) {
                return Err(DAGError::DuplicateFileUUID { uuid: out });
            }
        }
        if num_dependencies.insert(exec_uuid.clone(), count).is_some() {
            return Err(DAGError::DuplicateExecutionUUID {
                uuid: exec_uuid.clone(),
            });
        }
        if count == 0 {
            ready_execs.push_back(exec_uuid.clone());
        }
    }
    // add the provided files
    for uuid in dag.provided_files.keys() {
        ready_files.push_back(uuid.clone());
        if !known_files.insert(uuid.clone()) {
            return Err(DAGError::DuplicateFileUUID { uuid: uuid.clone() });
        }
    }
    // visit the DAG for finding the unreachable executions / cycles
    while !ready_execs.is_empty() || !ready_files.is_empty() {
        for file in ready_files.drain(..) {
            if !dependencies.contains_key(&file) {
                continue;
            }
            for exec in dependencies.get(&file).unwrap().iter() {
                let num_deps = num_dependencies
                    .get_mut(&exec)
                    .expect("num_dependencies of an unknown execution");
                assert_ne!(
                    *num_deps, 0,
                    "num_dependencis is going to be negative for {}",
                    exec
                );
                *num_deps -= 1;
                if *num_deps == 0 {
                    ready_execs.push_back(exec.clone());
                }
            }
        }
        for exec_uuid in ready_execs.drain(..) {
            let exec = dag.executions.get(&exec_uuid).expect("No such exec");
            for file in exec.outputs().into_iter() {
                ready_files.push_back(file);
            }
        }
    }
    // search for unreachable execution / cycles
    for (exec_uuid, count) in num_dependencies.iter() {
        if *count == 0 {
            continue;
        }
        let exec = dag.executions.get(&exec_uuid).unwrap();
        for dep in exec.dependencies().iter() {
            if !known_files.contains(dep) {
                return Err(DAGError::MissingFile {
                    uuid: *dep,
                    description: format!("Dependency of '{}'", exec.description),
                });
            }
        }
        return Err(DAGError::CycleDetected {
            description: exec.description.clone(),
        });
    }
    // check the file callbacks
    for file in callbacks.files.iter() {
        if !known_files.contains(&file) {
            return Err(DAGError::MissingFile {
                uuid: *file,
                description: format!("File required by a callback"),
            });
        }
    }
    // check the execution callbacks
    for exec in callbacks.executions.iter() {
        if !num_dependencies.contains_key(&exec) {
            return Err(DAGError::MissingExecution { uuid: *exec });
        }
    }
    Ok(())
}
