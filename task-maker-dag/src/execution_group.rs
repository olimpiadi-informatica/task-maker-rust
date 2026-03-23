use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Execution, ExecutionDAGConfig, ExecutionInputBehaviour, ExecutionOutputBehaviour, ExecutionTag,
    FileUuid, Priority,
};

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

/// Settings for execution groups that use a controller.
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
pub struct ControllerSettings {
    /// Upper bound on the number of processes that the controller can ask to spawn.
    pub process_limit: usize,
    /// Whether to assume that the solutions were spawned concurrently or not.
    pub concurrent: bool,
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
    /// The configuration of the underlying DAG. Will be overwritten by
    /// `ExecutionDAG.add_execution`.
    pub config: ExecutionDAGConfig,
    /// The tag associated with this execution.
    pub tag: Option<ExecutionTag>,
    /// A priority index for this execution. Higher values correspond to higher priorities. The
    /// priority order is followed only between ready executions, i.e. a lower priority one can be
    /// executed before if its dependencies are ready earlier.
    pub priority: Priority,
    /// If `Some`, this execution group uses a controller. In that case, the
    /// first element of `executions` will represent the command to launch the
    /// controller, and the second element will represent the *prefix* of the
    /// command to launch solutions (that is, the controller can append extra
    /// command line arguments).
    /// If using a controller, `fifo`s are ignored, executions cannot be typst
    /// compilations, and stdin/stdout behaviour must be set to "Ignored"
    /// for all executions.
    pub controller_settings: Option<ControllerSettings>,
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
            config: ExecutionDAGConfig::new(),
            priority: Priority::default(),
            tag: None,
            controller_settings: None,
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

    /// List of all the [File](struct.File.html) dependencies of the execution
    /// group, including `stdin`.
    pub fn dependencies(&self) -> Vec<FileUuid> {
        let mut deps = vec![];
        for exec in &self.executions {
            if let ExecutionInputBehaviour::File(stdin) = exec.stdin {
                deps.push(stdin);
            }
            for input in exec.input_files.values() {
                deps.push(input.file);
            }
        }
        deps
    }

    /// List of all the [File](struct.File.html) produced by the execution
    /// group, including `stdout` and `stderr`.
    pub fn outputs(&self) -> Vec<FileUuid> {
        let mut outs = vec![];
        for exec in &self.executions {
            if let ExecutionOutputBehaviour::Capture { file: stdout, .. } = &exec.stdout {
                outs.push(stdout.uuid);
            }
            if let ExecutionOutputBehaviour::Capture { file: stderr, .. } = &exec.stderr {
                outs.push(stderr.uuid);
            }
            for output in exec.output_files.values() {
                outs.push(output.uuid);
            }
        }
        outs
    }
}

impl From<Execution> for ExecutionGroup {
    fn from(exec: Execution) -> Self {
        let mut group = ExecutionGroup::new(exec.description.clone());
        group.add_execution(exec);
        group
    }
}
