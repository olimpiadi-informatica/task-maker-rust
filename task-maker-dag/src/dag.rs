use crate::file::*;
use crate::*;
use boxfnonce::BoxFnOnce;
use failure::{Error, Fail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use task_maker_store::*;

/// A wrapper around a File provided by the client, this means that the client
/// knows the FileStoreKey and the path to that file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidedFile {
    /// The file handle.
    pub file: File,
    /// The key in the FileStore.
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
}

/// List of the _interesting_ files and executions, only the callbacks listed here will be called by
/// the server. Every other callback is not sent to the client for performance reasons.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGCallbacks {
    /// Set of the handles of the executions that have at least a callback bound.
    pub executions: HashSet<ExecutionUuid>,
    /// Set of the handles of the files that have at least a callback bound.
    pub files: HashSet<FileUuid>,
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

/// An error in the DAG structure.
#[derive(Debug, Fail)]
pub enum DAGError {
    /// A file is used as input in an execution but it's missing, or a callback is registered on a
    /// file but it's missing.
    #[fail(display = "missing file {} ({})", description, uuid)]
    MissingFile { uuid: FileUuid, description: String },
    /// A callback is registered on an execution but it's missing.
    #[fail(display = "missing execution {}", uuid)]
    MissingExecution { uuid: FileUuid },
    /// There is a dependency cycle in the DAG.
    #[fail(
        display = "detected dependency cycle, '{}' is in the cycle",
        description
    )]
    CycleDetected { description: String },
    /// There is a duplicate execution UUID.
    #[fail(display = "duplicate execution UUID {}", uuid)]
    DuplicateExecutionUUID { uuid: ExecutionUuid },
    /// There is a duplicate file UUID.
    #[fail(display = "duplicate file UUID {}", uuid)]
    DuplicateFileUUID { uuid: FileUuid },
}

impl ExecutionDAG {
    /// Create an empty ExecutionDAG, without files and executions.
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
    pub fn add_execution(&mut self, execution: Execution) {
        self.data.executions.insert(execution.uuid, execution);
    }

    /// When `file` is ready it will be written to `path`. The file must be present in the dag
    /// before the evaluation starts.
    pub fn write_file_to<F: Into<FileUuid>, P: Into<PathBuf>>(&mut self, file: F, path: P) {
        self.file_callback(file.into()).write_to = Some(path.into());
    }

    /// Call `callback` with the first `limit` bytes of the file when it's
    /// ready. The file must be present in the DAG before the evaluation
    /// starts.
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

    /// Makes sure that a callback item exists for that file and returns a &mut to it.
    fn file_callback<F: Into<FileUuid>>(&mut self, file: F) -> &mut FileCallbacks {
        self.file_callbacks.entry(file.into()).or_default()
    }

    /// Makes sure that a callback item exists for that execution and returns a &mut to it.
    fn execution_callback(&mut self, execution: &ExecutionUuid) -> &mut ExecutionCallbacks {
        self.execution_callbacks.entry(*execution).or_default()
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
        dependencies
            .entry(file)
            .or_insert_with(|| vec![])
            .push(exec);
    };

    // add the executions and check for duplicated UUIDs
    for exec_uuid in dag.executions.keys() {
        let exec = dag.executions.get(exec_uuid).expect("No such exec");
        let deps = exec.dependencies();
        let count = deps.len();
        for dep in deps.into_iter() {
            add_dependency(dep, *exec_uuid);
        }
        for out in exec.outputs().into_iter() {
            if !known_files.insert(out) {
                return Err(DAGError::DuplicateFileUUID { uuid: out });
            }
        }
        if num_dependencies.insert(*exec_uuid, count).is_some() {
            return Err(DAGError::DuplicateExecutionUUID { uuid: *exec_uuid });
        }
        if count == 0 {
            ready_execs.push_back(exec_uuid.clone());
        }
    }
    // add the provided files
    for uuid in dag.provided_files.keys() {
        ready_files.push_back(uuid.clone());
        if !known_files.insert(uuid.clone()) {
            return Err(DAGError::DuplicateFileUUID { uuid: *uuid });
        }
    }
    // visit the DAG for finding the unreachable executions / cycles
    while !ready_execs.is_empty() || !ready_files.is_empty() {
        for file in ready_files.drain(..) {
            if !dependencies.contains_key(&file) {
                continue;
            }
            for exec in dependencies[&file].iter() {
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
        let exec = &dag.executions[&exec_uuid];
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
                description: "File required by a callback".to_owned(),
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
