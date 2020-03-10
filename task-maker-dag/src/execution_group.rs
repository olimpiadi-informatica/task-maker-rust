use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Execution, ExecutionDAGConfig, ExecutionTag, Priority};

/// The identifier of an execution group, it's globally unique and it identifies a group during an
/// evaluation.
#[derive(Debug, Clone, Copy, Hash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionGroupUuid(Uuid); // TODO revert to type alias

impl std::fmt::Display for ExecutionGroupUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A group of executions that have to be executed concurrently in the same worker. If any of the
/// executions crash, all the group is stopped. The executions inside the group can communicate
/// using FIFO pipes provided by the OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionGroup {
    /// The unique identifier of this group of executions.
    pub uuid: ExecutionGroupUuid,
    /// A textual description of the group.
    pub description: String,
    /// The list of executions to run.
    pub executions: Vec<Execution>,
    // /// The list of FIFO pipes to create for this group.
    // pub fifo: Vec<Fifo>,
}

impl ExecutionGroup {
    /// Create an empty execution group.
    pub fn new<S: Into<String>>(descr: S) -> ExecutionGroup {
        ExecutionGroup {
            uuid: ExecutionGroupUuid(Uuid::new_v4()),
            description: descr.into(),
            executions: vec![],
        }
    }

    /// Add a new execution to the group.
    pub fn add_execution(&mut self, exec: Execution) -> &mut Self {
        self.executions.push(exec);
        self
    }

    /// Priority of this execution group. The actual value is computed based on the executions
    /// contained in this group.
    pub fn priority(&self) -> Priority {
        self.executions
            .iter()
            .map(|e| e.priority)
            .max()
            .unwrap_or(0)
    }

    /// A reference to the configuration of the underlying DAG.
    pub fn config(&self) -> &ExecutionDAGConfig {
        self.executions
            .get(0)
            .expect("Invalid group with zero executions")
            .config()
    }

    /// The tag of one of the executions in this group.
    pub fn tag(&self) -> Option<ExecutionTag> {
        self.executions
            .get(0)
            .expect("Invalid group with zero executions")
            .tag
            .clone()
    }
}
