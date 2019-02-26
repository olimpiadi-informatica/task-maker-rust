use crate::execution::execution::*;
use crate::execution::file::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAG {
    pub provided_files: Vec<SharedFile>,
    pub executions: Vec<Rc<Execution>>,
}

impl ExecutionDAG {
    pub fn new() -> ExecutionDAG {
        ExecutionDAG {
            provided_files: vec![],
            executions: vec![],
        }
    }

    pub fn provide_file(self: &mut Self, file: SharedFile) {
        self.provided_files.push(file);
    }

    pub fn add_execution(self: &mut Self, execution: Rc<Execution>) {
        self.executions.push(execution);
    }

    pub fn execute(self) {
        unimplemented!();
    }
}
