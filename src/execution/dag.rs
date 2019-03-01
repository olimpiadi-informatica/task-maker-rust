use crate::execution::execution::*;
use crate::execution::file::*;
use failure::{Error, Fail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGData {
    pub provided_files: HashMap<Uuid, File>,
    pub executions: HashMap<Uuid, Execution>,
}

#[derive(Debug)]
pub struct ExecutionDAG {
    pub data: ExecutionDAGData,
    pub execution_callbacks: HashMap<Uuid, ExecutionCallbacks>,
    pub file_callbacks: HashMap<Uuid, FileCallbacks>,
}

#[derive(Debug, Fail)]
pub enum DAGError {
    #[fail(display = "missing file {} ({})", description, uuid)]
    MissingFile { uuid: Uuid, description: String },
    #[fail(
        display = "detected dependency cycle, '{}' is in the cycle",
        description
    )]
    CycleDetected { description: String },
    #[fail(display = "duplicate execution UUID {}", uuid)]
    DuplicateExecutionUUID { uuid: Uuid },
    #[fail(display = "duplicate file UUID {}", uuid)]
    DuplicateFileUUID { uuid: Uuid },
}

pub struct AddExecutionWrapper<'a> {
    uuid: Uuid,
    dag: &'a mut ExecutionDAG,
}

impl ExecutionDAG {
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

    pub fn provide_file(self: &mut Self, file: File) {
        self.data.provided_files.insert(file.uuid.clone(), file);
    }

    pub fn add_execution(self: &mut Self, execution: Execution) -> AddExecutionWrapper {
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
    pub fn on_start(mut self, callback: &'static OnStartCallback) -> AddExecutionWrapper<'a> {
        self.ensure_execution_callback().on_start = Some(Box::new(callback));
        self
    }

    pub fn on_done(mut self, callback: &'static OnDoneCallback) -> AddExecutionWrapper<'a> {
        self.ensure_execution_callback().on_done = Some(Box::new(callback));
        self
    }

    pub fn on_skip(mut self, callback: &'static OnSkipCallback) -> AddExecutionWrapper<'a> {
        self.ensure_execution_callback().on_skip = Some(Box::new(callback));
        self
    }

    pub fn write_stdout_to(mut self, path: &str) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stdout().uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    pub fn write_stderr_to(mut self, path: &str) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stderr().uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    pub fn write_output_to(mut self, output: &str, path: &str) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().output(output).uuid.clone();
        self.write_file_to(path, uuid);
        self
    }

    pub fn get_stdout_content(
        mut self,
        limit: usize,
        callback: &'static GetContentCallback,
    ) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stdout().uuid.clone();
        self.bind_get_content(limit, callback, uuid);
        self
    }

    pub fn get_stderr_content(
        mut self,
        limit: usize,
        callback: &'static GetContentCallback,
    ) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().stderr().uuid.clone();
        self.bind_get_content(limit, callback, uuid);
        self
    }

    pub fn get_output_content(
        mut self,
        output: &str,
        limit: usize,
        callback: &'static GetContentCallback,
    ) -> AddExecutionWrapper<'a> {
        let uuid = self.get_execution().output(output).uuid.clone();
        self.bind_get_content(limit, callback, uuid);
        self
    }

    fn write_file_to(&mut self, path: &str, uuid: Uuid) {
        self.ensure_file_callback(&uuid);
        self.dag.file_callbacks.get_mut(&uuid).unwrap().write_to = Some(path.to_owned());
    }

    fn bind_get_content(
        &mut self,
        limit: usize,
        callback: &'static GetContentCallback,
        uuid: Uuid,
    ) {
        self.ensure_file_callback(&uuid);
        self.dag.file_callbacks.get_mut(&uuid).unwrap().get_content =
            Some((limit, Box::new(callback)));
    }

    fn ensure_file_callback(&mut self, uuid: &Uuid) {
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

pub fn check_dag(dag: &ExecutionDAGData) -> Result<(), DAGError> {
    let mut dependencies: HashMap<Uuid, Vec<Uuid>> = HashMap::new(); // FileUuid -> [ExecUuid]
    let mut num_dependencies: HashMap<Uuid, usize> = HashMap::new(); // ExecUuid -> count
    let mut known_files: HashSet<Uuid> = HashSet::new();
    let mut ready_execs: VecDeque<Uuid> = VecDeque::new();
    let mut ready_files: VecDeque<Uuid> = VecDeque::new();

    let mut add_dependency = |file: Uuid, exec: Uuid| {
        if !dependencies.contains_key(&file) {
            dependencies.insert(file, vec![exec]);
        } else {
            dependencies.get_mut(&file).unwrap().push(exec);
        }
    };

    let exec_dependencies = |exec: &Uuid| {
        let mut deps = vec![];
        let exec = dag.executions.get(exec).expect("No such exec");
        if let Some(stdin) = exec.stdin {
            deps.push(stdin);
        }
        for input in exec.inputs.iter() {
            deps.push(input.file);
        }
        deps
    };

    let exec_outputs = |exec: &Uuid| {
        let mut outs = vec![];
        let exec = dag.executions.get(exec).expect("No such exec");
        if let Some(stdout) = &exec.stdout {
            outs.push(stdout.uuid.clone());
        }
        if let Some(stderr) = &exec.stderr {
            outs.push(stderr.uuid.clone());
        }
        for output in exec.outputs.values() {
            outs.push(output.uuid.clone());
        }
        outs
    };

    for exec_uuid in dag.executions.keys() {
        let deps = exec_dependencies(&exec_uuid);
        let count = deps.len();
        for dep in deps.into_iter() {
            add_dependency(dep, exec_uuid.clone());
        }
        for out in exec_outputs(&exec_uuid).into_iter() {
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
    for uuid in dag.provided_files.keys() {
        ready_files.push_back(uuid.clone());
        if !known_files.insert(uuid.clone()) {
            return Err(DAGError::DuplicateFileUUID { uuid: uuid.clone() });
        }
    }
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
        for exec in ready_execs.drain(..) {
            for file in exec_outputs(&exec).into_iter() {
                ready_files.push_back(file);
            }
        }
    }
    for (exec_uuid, count) in num_dependencies.iter() {
        if *count == 0 {
            continue;
        }
        let exec = dag.executions.get(&exec_uuid).unwrap();
        for dep in exec_dependencies(exec_uuid).iter() {
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
    Ok(())
}
