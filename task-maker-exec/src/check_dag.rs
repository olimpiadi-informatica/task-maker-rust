use crate::ExecutionDAGWatchSet;
use failure::Fail;
use std::collections::{HashMap, HashSet, VecDeque};
use task_maker_dag::{ExecutionDAGData, ExecutionUuid, FileUuid};

/// An error in the DAG structure.
#[derive(Debug, Fail)]
pub enum DAGError {
    /// A file is used as input in an execution but it's missing, or a callback is registered on a
    /// file but it's missing.
    #[fail(display = "missing file {} ({})", description, uuid)]
    MissingFile {
        /// The UUID of the missing file.
        uuid: FileUuid,
        /// The description of the missing file.
        description: String,
    },
    /// A callback is registered on an execution but it's missing.
    #[fail(display = "missing execution {}", uuid)]
    MissingExecution {
        /// The UUID of the missing execution.
        uuid: ExecutionUuid,
    },
    /// There is a dependency cycle in the DAG.
    #[fail(
        display = "detected dependency cycle, '{}' is in the cycle",
        description
    )]
    CycleDetected {
        /// The description of an execution inside the cycle.
        description: String,
    },
    /// There is a duplicate file UUID.
    #[fail(display = "duplicate file UUID {}", uuid)]
    DuplicateFileUUID {
        /// The duplicated UUID.
        uuid: FileUuid,
    },
}

/// Validate the DAG checking if all the required pieces are present and they actually make a DAG.
/// It's checked that no duplicated UUID are present, no files are missing, all the executions are
/// reachable and no cycles are present.
pub fn check_dag(dag: &ExecutionDAGData, callbacks: &ExecutionDAGWatchSet) -> Result<(), DAGError> {
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
        num_dependencies.insert(*exec_uuid, count);
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

#[cfg(test)]
mod tests {
    use super::*;
    use task_maker_dag::{Execution, ExecutionCommand, ExecutionDAG, File};

    #[test]
    fn test_missing_file() {
        let mut dag = ExecutionDAG::new();
        let mut exec = Execution::new("exec", ExecutionCommand::local("foo"));
        let file = File::new("file");
        exec.stdin(file.clone());
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
        exec2.stdout = Some(file.clone());
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
