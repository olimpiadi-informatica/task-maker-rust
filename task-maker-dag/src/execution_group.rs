use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Execution, ExecutionDAGConfig, ExecutionTag, Priority};

/// Directory inside the sandbox where to place all the pipes of the group. This is used to allow
/// the sandbox bind-mount all the pipes with a single mount point, inside all the sandboxes of the
/// group.
pub static FIFO_SANDBOX_DIR: &str = "tm_pipes";

/// The identifier of an execution group, it's globally unique and it identifies a group during an
/// evaluation.
pub type ExecutionGroupUuid = Uuid;
/// The identifier of a Fifo pipe inside a group.
pub type FifoUuid = Uuid;

/// A First-in First-out channel for letting executions communicate inside an execution group. Each
/// Fifo is identified by an UUID which is unique inside the same `ExecutionGroup`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Fifo {
    /// The UUID of this `Fifo`.
    pub uuid: FifoUuid,
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
    /// The list of FIFO pipes to create for this group.
    pub fifo: Vec<Fifo>,
}

impl Fifo {
    /// Make a new Fifo with a random uuid.
    fn new() -> Fifo {
        Fifo {
            uuid: Uuid::new_v4(),
        }
    }

    /// The path inside the sandbox that this pipe is mapped to.
    pub fn sandbox_path(&self) -> PathBuf {
        Path::new(FIFO_SANDBOX_DIR).join(self.uuid.to_string())
    }
}

impl ExecutionGroup {
    /// Create an empty execution group.
    pub fn new<S: Into<String>>(descr: S) -> ExecutionGroup {
        ExecutionGroup {
            uuid: Uuid::new_v4(),
            description: descr.into(),
            executions: vec![],
            fifo: vec![],
        }
    }

    /// Add a new execution to the group.
    pub fn add_execution(&mut self, exec: Execution) -> &mut Self {
        self.executions.push(exec);
        self
    }

    /// Create a new `Fifo` and return it.
    pub fn new_fifo(&mut self) -> Fifo {
        let fifo = Fifo::new();
        self.fifo.push(fifo);
        fifo
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
            .first()
            .expect("Invalid group with zero executions")
            .config()
    }

    /// The tag of one of the executions in this group.
    pub fn tag(&self) -> Option<ExecutionTag> {
        self.executions
            .first()
            .expect("Invalid group with zero executions")
            .tag
            .clone()
    }
}

impl From<Execution> for ExecutionGroup {
    fn from(exec: Execution) -> Self {
        let mut group = ExecutionGroup::new(exec.description.clone());
        group.add_execution(exec);
        group
    }
}
