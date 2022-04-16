#![allow(dead_code)]

use crate::file_set::FileSetFile;
use crate::store::{DataIdentificationHash, FileSetHash, VariantIdentificationHash};
use bincode::serialize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use task_maker_dag::ExecutionCommand;

/// Higher value = executed first.
type Priority = i64;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGOptions {
    pub keep_sandboxes: bool,
    pub priority: Priority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Limit on the userspace cpu time of the process.
    pub cpu_time: Option<Duration>,
    /// Limit on the kernels pace cpu time of the process.
    pub sys_time: Option<Duration>,
    /// Limit on the total time of execution. This will include the io-wait time and
    /// other non-cpu times.
    pub wall_time: Option<Duration>,
    /// Additional time after the time limit before killing the process.
    pub extra_time: Option<Duration>,
    /// Limit on the number of KiB the process can use in any moment. This can be page-aligned by
    /// the sandbox.
    pub memory: Option<u64>,
    /// Limit on the number of threads/processes the process can spawn.
    pub nproc: Option<u32>,
    /// Limit on the number of file descriptors the process can keep open.
    pub nofile: Option<u32>,
    /// Maximum size of the files (in bytes) the process can write/create.
    pub fsize: Option<u64>,
    /// RLIMIT_MEMLOCK
    pub memlock: Option<u64>,
    /// Limit on the stack size for the process in KiB.
    pub stack: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionConstraints {
    /// Whether the process in the sandbox is not allowed to create new files inside the sandbox.
    pub read_only: bool,
    /// Whether the process in the sandbox can use `/dev/null` and `/tmp`.
    pub mount_tmpfs: bool,
    /// Whether the process in the sandbox can use `/proc`.
    pub mount_proc: bool,
    /// Extra directory that can be read inside the sandbox.
    pub extra_readable_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionPath {
    Stdin,
    Stdout,
    Stderr,
    Path(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputFilePermissions {
    Default,
    Executable,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecutionInputFileInfo {
    pub hash: FileSetHash,
    pub file_id: FileSetFile,
    pub permissions: InputFilePermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionFileMode {
    /// Input file to be read from the store.
    Input(ExecutionInputFileInfo),
    /// Output file to be written to the store.
    Output,
    /// Fifo; executions within the same execution group that share the same Fifo name will share
    /// the Fifo.
    Fifo(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    /// Name of the execution, that identifies it within its execution group and is used to refer
    /// to it.
    pub name: String,
    /// Which command to execute.
    pub command: ExecutionCommand,
    /// The list of command line arguments.
    pub args: Vec<String>,

    /// All files (input, output and fifo) that are necessary for running this execution.
    /// ExecutionPath must not be repeated.
    pub files: Vec<(ExecutionPath, ExecutionFileMode)>,

    /// Environment variables to set (list of (Key, Value) pairs). The key must not be repeated.
    pub env: Vec<(String, String)>,
    /// Environment variables to copy from the sandbox host.
    pub copy_env: Vec<String>,

    /// Limits on the execution.
    pub limits: ExecutionLimits,

    /// Other constraints on the execution
    pub constraints: ExecutionConstraints,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionGroup {
    /// A textual description of the group.
    pub description: String,

    /// The list of executions to run.
    pub executions: Vec<Execution>,

    /// Setting this field to non-None will cause the execution to skip caching. A different string
    /// is required for every run in which skipping the cache is desired.
    pub skip_cache_key: Option<String>,

    /// A priority index for this execution group.
    pub priority: Priority,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAG {
    pub execution_groups: Vec<ExecutionGroup>,
}

impl ExecutionGroup {
    pub fn get_data_identification_hash(&self) -> DataIdentificationHash {
        let mut hasher = blake3::Hasher::new();
        for ex in self.executions.iter() {
            hasher.update(ex.name.as_bytes());
            hasher.update(&serialize(&ex.command).unwrap());
            hasher.update(&serialize(&ex.args).unwrap());
            for (path, file) in ex.files.iter() {
                hasher.update(&serialize(path).unwrap());
                match file {
                    ExecutionFileMode::Input(ExecutionInputFileInfo {
                        hash,
                        file_id,
                        permissions,
                    }) => {
                        hasher.update(&serialize(&hash.data).unwrap());
                        hasher.update(&serialize(file_id).unwrap());
                        hasher.update(&serialize(permissions).unwrap());
                    }
                    _ => {
                        hasher.update(&serialize(file).unwrap());
                    }
                }
            }
            hasher.update(&serialize(&ex.constraints).unwrap());
        }
        *hasher.finalize().as_bytes()
    }
    pub fn get_variant_identification_hash(&self) -> VariantIdentificationHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.description.as_bytes());
        hasher.update(&serialize(&self.executions).unwrap());
        hasher.update(&serialize(&self.skip_cache_key).unwrap());
        *hasher.finalize().as_bytes()
    }
}
