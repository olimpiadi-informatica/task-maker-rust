use crate::execution::execution::*;
use crate::execution::file::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
