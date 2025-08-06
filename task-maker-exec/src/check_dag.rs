use crate::executor::ExecutionDAGWatchSet;
use std::collections::{HashMap, HashSet, VecDeque};
use task_maker_dag::{ExecutionDAGData, ExecutionGroupUuid, ExecutionUuid, FifoUuid, FileUuid};
use thiserror::Error;

/// An error in the DAG structure.
#[derive(Debug, Error)]
pub enum DAGError {
    /// A file is used as input in an execution but it's missing, or a callback is registered on a
    /// file but it's missing.
    #[error("missing file {description} ({uuid})")]
    MissingFile {
        /// The UUID of the missing file.
        uuid: FileUuid,
        /// The description of the missing file.
        description: String,
    },
    /// Stdout/Stderr capture is requested, but a UUID for them is missing.
    #[error("missing UUID for captured {stream} on execution {uuid} ({description})")]
    InvalidCapture {
        /// Either "stdout" or "stderr".
        stream: String,
        /// The UUID of the missing file.
        uuid: ExecutionUuid,
        /// The description of the missing file.
        description: String,
    },
    /// A callback is registered on an execution but it's missing.
    #[error("missing execution {uuid}")]
    MissingExecution {
        /// The UUID of the missing execution.
        uuid: ExecutionUuid,
    },
    /// There is a dependency cycle in the DAG.
    #[error("detected dependency cycle, '{description}' is in the cycle")]
    CycleDetected {
        /// The description of an execution inside the cycle.
        description: String,
    },
    /// There is a duplicate file UUID.
    #[error("duplicate file UUID {uuid}")]
    DuplicateFileUUID {
        /// The duplicated UUID.
        uuid: FileUuid,
    },
    /// There is a duplicate Fifo UUID.
    #[error("duplicate FIFO UUID {uuid}")]
    DuplicateFifoUUID {
        /// The duplicated UUID.
        uuid: FifoUuid,
    },
    /// There is a duplicate execution UUID.
    #[error("duplicate execution UUID {uuid}")]
    DuplicateExecutionUUID {
        /// The duplicated UUID.
        uuid: FileUuid,
    },
    /// There is an invalid execution group.
    #[error("empty execution group {uuid}")]
    EmptyGroup {
        /// The UUID of the execution group.
        uuid: ExecutionGroupUuid,
    },
}

/// Validate the DAG checking if all the required pieces are present and they actually make a DAG.
/// It's checked that no duplicated UUID are present, no files are missing, all the executions are
/// reachable and no cycles are present.
pub fn check_dag(dag: &ExecutionDAGData, callbacks: &ExecutionDAGWatchSet) -> Result<(), DAGError> {
    let mut dependencies: HashMap<FileUuid, Vec<ExecutionGroupUuid>> = HashMap::new();
    let mut num_dependencies: HashMap<ExecutionGroupUuid, usize> = HashMap::new();
    let mut known_files: HashSet<FileUuid> = HashSet::new();
    let mut known_execs: HashSet<ExecutionUuid> = HashSet::new();
    let mut ready_groups: VecDeque<ExecutionGroupUuid> = VecDeque::new();
    let mut ready_files: VecDeque<FileUuid> = VecDeque::new();

    let mut add_dependency = |file: FileUuid, group: ExecutionGroupUuid| {
        dependencies.entry(file).or_default().push(group);
    };

    // add the executions and check for duplicated UUIDs
    for (group_uuid, group) in dag.execution_groups.iter() {
        if group.executions.is_empty() {
            return Err(DAGError::EmptyGroup { uuid: *group_uuid });
        }
        let mut fifo_uuids = HashSet::new();
        for fifo in group.fifo.iter() {
            if !fifo_uuids.insert(fifo.uuid) {
                return Err(DAGError::DuplicateFifoUUID { uuid: fifo.uuid });
            }
        }
        let mut count = 0;
        for exec in &group.executions {
            let deps = exec.dependencies();
            if !known_execs.insert(exec.uuid) {
                return Err(DAGError::DuplicateExecutionUUID { uuid: exec.uuid });
            }
            count += deps.len();
            for dep in deps.into_iter() {
                add_dependency(dep, *group_uuid);
            }
            if exec.capture_stdout.is_some() && exec.stdout.is_none() {
                return Err(DAGError::InvalidCapture {
                    stream: "stdout".to_string(),
                    uuid: exec.uuid,
                    description: exec.description.clone(),
                });
            }
            if exec.capture_stderr.is_some() && exec.stderr.is_none() {
                return Err(DAGError::InvalidCapture {
                    stream: "stderr".to_string(),
                    uuid: exec.uuid,
                    description: exec.description.clone(),
                });
            }
            for out in exec.outputs().into_iter() {
                if !known_files.insert(out) {
                    return Err(DAGError::DuplicateFileUUID { uuid: out });
                }
            }
        }
        num_dependencies.insert(*group_uuid, count);
        if count == 0 {
            ready_groups.push_back(*group_uuid);
        }
    }
    // add the provided files
    for uuid in dag.provided_files.keys() {
        ready_files.push_back(*uuid);
        if !known_files.insert(*uuid) {
            return Err(DAGError::DuplicateFileUUID { uuid: *uuid });
        }
    }
    // visit the DAG for finding the unreachable executions / cycles
    while !ready_groups.is_empty() || !ready_files.is_empty() {
        for file in ready_files.drain(..) {
            if !dependencies.contains_key(&file) {
                continue;
            }
            for group_uuid in dependencies[&file].iter() {
                let num_deps = num_dependencies
                    .get_mut(group_uuid)
                    .expect("num_dependencies of an unknown execution group");
                assert_ne!(
                    *num_deps, 0,
                    "num_dependencies is going to be negative for {group_uuid}"
                );
                *num_deps -= 1;
                if *num_deps == 0 {
                    ready_groups.push_back(*group_uuid);
                }
            }
        }
        for group_uuid in ready_groups.drain(..) {
            let group = dag
                .execution_groups
                .get(&group_uuid)
                .expect("No such exec group");
            for exec in &group.executions {
                for file in exec.outputs().into_iter() {
                    ready_files.push_back(file);
                }
            }
        }
    }
    // search for unreachable execution / cycles
    for (group_uuid, count) in num_dependencies.iter() {
        if *count == 0 {
            continue;
        }
        let group = &dag.execution_groups[group_uuid];
        for exec in &group.executions {
            for dep in exec.dependencies().iter() {
                if !known_files.contains(dep) {
                    return Err(DAGError::MissingFile {
                        uuid: *dep,
                        description: format!("Dependency of '{}'", exec.description),
                    });
                }
            }
        }
        return Err(DAGError::CycleDetected {
            description: dag.execution_groups[group_uuid].description.clone(),
        });
    }
    // check the file callbacks
    for file in callbacks.files.iter() {
        if !known_files.contains(file) {
            return Err(DAGError::MissingFile {
                uuid: *file,
                description: "File required by a callback".to_owned(),
            });
        }
    }
    // check the execution callbacks
    for exec in callbacks.executions.iter() {
        if !known_execs.contains(exec) {
            return Err(DAGError::MissingExecution { uuid: *exec });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use task_maker_dag::{Execution, ExecutionCommand, ExecutionDAG, File};

    #[test]
    fn test_missing_file() {
        let mut dag = ExecutionDAG::new();
        let mut exec = Execution::new("exec", ExecutionCommand::local("foo"));
        let file = File::new("file");
        exec.stdin(file);
        dag.add_execution(exec);
        assert!(check_dag(&dag.data, &ExecutionDAGWatchSet::default()).is_err());
    }

    #[test]
    fn test_missing_file_callback() {
        let dag = ExecutionDAG::new();
        let file = File::new("file");
        let watch = ExecutionDAGWatchSet {
            executions: Default::default(),
            files: [file.uuid].iter().cloned().collect(),
            urgent_files: Default::default(),
        };
        assert!(check_dag(&dag.data, &watch).is_err());
    }

    #[test]
    fn test_missing_execution_callback() {
        let dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        let watch = ExecutionDAGWatchSet {
            executions: [exec.uuid].iter().cloned().collect(),
            files: Default::default(),
            urgent_files: Default::default(),
        };
        assert!(check_dag(&dag.data, &watch).is_err());
    }

    #[test]
    fn test_cycle_self() {
        let mut dag = ExecutionDAG::new();
        let mut exec = Execution::new("exec", ExecutionCommand::local("foo"));
        let stdout = exec.stdout();
        exec.stdin(stdout);
        dag.add_execution(exec);
        assert!(check_dag(&dag.data, &ExecutionDAGWatchSet::default()).is_err());
    }

    #[test]
    fn test_cycle_double() {
        let mut dag = ExecutionDAG::new();
        let mut exec1 = Execution::new("exec", ExecutionCommand::local("foo"));
        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec1.stdin(exec2.stdout());
        exec2.stdin(exec1.stdout());
        dag.add_execution(exec1);
        dag.add_execution(exec2);
        assert!(check_dag(&dag.data, &ExecutionDAGWatchSet::default()).is_err());
    }

    #[test]
    fn test_duplicate_file() {
        let mut dag = ExecutionDAG::new();
        let mut exec1 = Execution::new("exec", ExecutionCommand::local("foo"));
        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        let file = File::new("file");
        exec1.stdout = Some(file.clone());
        exec2.stdout = Some(file);
        dag.add_execution(exec1);
        dag.add_execution(exec2);
        assert!(check_dag(&dag.data, &ExecutionDAGWatchSet::default()).is_err());
    }

    #[test]
    fn test_duplicate_file_provided() {
        let mut dag = ExecutionDAG::new();
        let mut exec = Execution::new("exec", ExecutionCommand::local("foo"));
        let file = exec.stdout();
        dag.add_execution(exec);
        dag.provide_file(file, "/dev/null").unwrap();
        assert!(check_dag(&dag.data, &ExecutionDAGWatchSet::default()).is_err());
    }
}
