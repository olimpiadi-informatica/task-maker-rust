//! Crate for managing the cache of the executions of a DAG.
//!
//! It provides the `Cache` struct which holds the cache data and stores it on disk on Drop. The
//! executions are cached computing a cache key based on the execution command, arguments and
//! inputs. For each cache key there may be more than one cache entry, allowing different execution
//! limits to be used.
//!
//! The algorithm for extending a cache entry for a different limit is the following:
//! - call `E1` the cached execution's result and `L1` its limits
//! - call `E2` the execution to check and `L2` its limits
//! - if `E1` was successful with `L1` and `L2` is _less restrictive_ than `L1`, `E2` will be
//!   successful
//! - if `E1` wasn't successful with `L1` and `L1` is _less restrictive_ than `L2`, `E2` won't be
//!   successful
//!
//! `L1` is _less restrictive_ than `L2` if there is no limit on `L1` that is _more restrictive_
//! than the corresponding one in `L2`. If a limit is not present, its value is assumed to be
//! _infinite_.
//!
//! # Example
//!
//! ```
//! use tempdir::TempDir;
//! use task_maker_cache::{Cache, CacheResult};
//! use std::collections::HashMap;
//! use task_maker_dag::{Execution, ExecutionCommand, ExecutionResult, ExecutionStatus, ExecutionResourcesUsage, File};
//! use task_maker_store::{FileStore, FileStoreKey};
//!
//! // make a new store and a new cache in a testing environment
//! let dir = TempDir::new("tm-test").unwrap();
//! let mut cache = Cache::new(dir.path()).expect("Cannot create the cache");
//! let mut store = FileStore::new(dir.path()).expect("Cannot create the store");
//!
//! // setup a testing file
//! let path = dir.path().join("file.txt");
//! std::fs::write(&path, [1, 2, 3, 4]).unwrap();
//!
//! // build a testing execution
//! let mut exec = Execution::new("Testing exec", ExecutionCommand::System("true".into()));
//! let input = File::new("Input file");
//! exec.input(&input, "sandbox_path", false);
//!
//! // emulate the execution
//! let result = ExecutionResult {
//!     status: ExecutionStatus::Success,
//!     resources: ExecutionResourcesUsage {
//!         cpu_time: 1.123,
//!         sys_time: 0.2,
//!         wall_time: 1.5,
//!         memory: 12345
//!     }
//! };
//!
//! // make the FileUuid -> FileStoreKey map
//! let key = FileStoreKey::from_file(&path).unwrap();
//! let mut file_keys = HashMap::new();
//! file_keys.insert(input.uuid, key);
//!
//! // insert the result in the cache
//! cache.insert(&exec, &file_keys, result, None, None, HashMap::new());
//!
//! // retrieve the result from the cache
//! let res = cache.get(&exec, &file_keys, &mut store);
//! match res {
//!     CacheResult::Miss => panic!("Expecting a hit"),
//!     CacheResult::Hit { result, outputs } => {
//!         assert_eq!(result.status, ExecutionStatus::Success);
//!         assert_eq!(result.resources.memory, 12345);
//!     }
//! }
//! ```

#![deny(missing_docs)]

#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use failure::Error;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use task_maker_dag::{
    Execution, ExecutionCommand, ExecutionLimits, ExecutionResult, ExecutionStatus, FileUuid,
};
use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey};

/// The name of the file which holds the cache data.
const CACHE_FILE: &str = "cache.json";

/// The cache key used to address the cache entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct CacheKey {
    /// The command of the execution. Note that this assumes that the system commands are all the
    /// same between the different workers.
    command: ExecutionCommand,
    /// The list of command line arguments.
    args: Vec<String>,
    /// The key (aka the hash) of the stdin, if any.
    stdin: Option<FileStoreKey>,
    /// The key (aka the hash) of the input files, and if they are executable. Note that because the
    /// order matters here (it changes the final hash of the key) those values are sorted
    /// lexicographically.
    inputs: Vec<(PathBuf, FileStoreKey, bool)>,
}

/// A cache entry for a given cache key. Note that the result will be used only if:
/// - all the required output files are still valid (ie inside the `FileStore`).
/// - the limits are compatible with the limits of the query.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CacheEntry {
    /// The result of the `Execution`.
    result: ExecutionResult,
    /// The limits associated with this entry.
    limits: ExecutionLimits,
    /// The key (aka the hash) of the stdout, if any.
    stdout: Option<FileStoreKey>,
    /// The key (aka the hash) of the stderr, if any.
    stderr: Option<FileStoreKey>,
    /// The key (aka the hash) of the output files, indexed by their path inside the sandbox.
    outputs: HashMap<PathBuf, FileStoreKey>,
}

/// The set of outputs produced by a cached execution. Note that this contains `FileStoreHandle`
/// that should be kept alive for preserving the file inside the `FileStore`.
#[derive(Debug)]
pub struct Outputs {
    /// The handle to the stdout, if any.
    pub stdout: Option<FileStoreHandle>,
    /// The handle to the stderr, if any.
    pub stderr: Option<FileStoreHandle>,
    /// The handle to the output files, indexed by their path inside the sandbox.
    pub outputs: HashMap<PathBuf, FileStoreHandle>,
}

/// Handle the cached executions, loading and storing them to disk.
#[derive(Debug)]
pub struct Cache {
    /// All the cached entries.
    entries: HashMap<CacheKey, Vec<CacheEntry>>,
    /// The path to the cache file.
    cache_file: PathBuf,
}

/// The result of a cache query, can be either successful (`Hit`) or unsuccessful (`Miss`).
pub enum CacheResult {
    /// The requested entry is not present in the cache.
    Miss,
    /// The requested entry is present in the cache.
    Hit {
        /// The result of the execution.
        result: ExecutionResult,
        /// The outputs of the execution.
        outputs: Outputs,
    },
}

impl CacheKey {
    /// Make a new `CacheKey` based on an `Execution` and on the mapping of its input files, from
    /// the UUIDs of the current DAG to the persisted `FileStoreKey`s.
    fn from_execution(
        execution: &Execution,
        file_keys: &HashMap<FileUuid, FileStoreKey>,
    ) -> CacheKey {
        let stdin = execution.stdin.as_ref().map(|f| file_keys[f].clone());
        let inputs = execution
            .inputs
            .clone()
            .into_iter()
            .map(|(p, f)| (p, file_keys[&f.file].clone(), f.executable))
            .sorted()
            .collect_vec();
        CacheKey {
            command: execution.command.clone(),
            args: execution.args.clone(),
            stdin,
            inputs,
        }
    }
}

impl CacheEntry {
    /// Search in the file store the handles of all the output files. Will return `None` if at least
    /// one of them is missing.
    fn outputs(&self, file_store: &mut FileStore) -> Option<Outputs> {
        // given an Option<FileStoreKey> will extract its FileStoreHandle if present, otherwise will
        // return None. If None is passed None is returned.
        macro_rules! try_get {
            ($key:expr) => {
                match &$key {
                    None => None,
                    Some(key) => match file_store.get(key) {
                        None => {
                            debug!("File {} is gone", key.to_string());
                            return None;
                        }
                        Some(handle) => Some(handle),
                    },
                }
            };
        }

        let stdout = try_get!(self.stdout);
        let stderr = try_get!(self.stderr);
        let mut outputs = HashMap::new();
        for (path, key) in self.outputs.iter() {
            outputs.insert(path.clone(), try_get!(Some(key)).unwrap());
        }
        Some(Outputs {
            stdout,
            stderr,
            outputs,
        })
    }

    /// Checks whether a given execution is compatible with the limits stored in this entry. See the
    /// docs of the crate for the definition of _compatible_.
    fn is_compatible(&self, execution: &Execution) -> bool {
        // makes sure that $left <= $right where None = inf
        macro_rules! check_limit {
            ($left:expr, $right:expr) => {
                match ($left, $right) {
                    (Some(left), Some(right)) => {
                        if !(left <= right) {
                            return false;
                        }
                    }
                    (None, Some(_)) => return false,
                    _ => {}
                }
            };
        }
        macro_rules! check_limits {
            ($left:expr, $right:expr) => {
                check_limit!($left.cpu_time, $right.cpu_time);
                check_limit!($left.sys_time, $right.sys_time);
                check_limit!($left.wall_time, $right.wall_time);
                check_limit!($left.memory, $right.memory);
                check_limit!($left.nproc, $right.nproc);
                check_limit!($left.nofile, $right.nofile);
                check_limit!($left.fsize, $right.fsize);
                check_limit!($left.memlock, $right.memlock);
                check_limit!($left.stack, $right.stack);
            };
        }
        match self.result.status {
            ExecutionStatus::Success => {
                // require that the new limits are less restrictive
                check_limits!(self.limits, execution.limits);
            }
            _ => {
                // require that the new limits are more restrictive
                check_limits!(execution.limits, self.limits);
            }
        }
        true
    }
}

impl Cache {
    /// Make a new `Cache` stored in the specified cache directory. If the cache file is present
    /// it will be used and its content will be loaded, if valid, otherwise an error is returned.
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Cache, Error> {
        let path = cache_dir.as_ref().join(CACHE_FILE);
        if path.exists() {
            let file = std::fs::File::open(&path)?;
            let entries: Vec<(CacheKey, Vec<CacheEntry>)> = serde_json::from_reader(file)?;
            Ok(Cache {
                entries: entries.into_iter().collect(),
                cache_file: path,
            })
        } else {
            Ok(Cache {
                entries: HashMap::new(),
                cache_file: path,
            })
        }
    }

    /// Insert a new entry inside the cache. They key is computed based on the execution's metadata
    /// and on the hash of it's inputs, defined by the mapping `file_keys` from the UUIDs of the DAG
    /// to the persistent `FileStoreKey`s.
    pub fn insert(
        &mut self,
        execution: &Execution,
        file_keys: &HashMap<FileUuid, FileStoreKey>,
        result: ExecutionResult,
        stdout: Option<FileStoreKey>,
        stderr: Option<FileStoreKey>,
        outputs: HashMap<PathBuf, FileStoreKey>,
    ) {
        let key = CacheKey::from_execution(execution, file_keys);
        let set = self.entries.entry(key.clone()).or_default();
        let entry = CacheEntry {
            result,
            limits: execution.limits.clone(),
            stdout,
            stderr,
            outputs,
        };
        // do not insert duplicated keys
        if set.iter().any(|e| e.limits == entry.limits) {
            return;
        }
        set.push(entry);
    }

    /// Search in the cache for a valid entry, returning a cache hit if it's found or a cache miss
    /// if not.
    ///
    /// The result contains the handles to the files in the `FileStore`, preventing the flushing
    /// from erasing them.
    pub fn get(
        &mut self,
        execution: &Execution,
        file_keys: &HashMap<FileUuid, FileStoreKey>,
        file_store: &mut FileStore,
    ) -> CacheResult {
        let key = CacheKey::from_execution(execution, file_keys);
        if !self.entries.contains_key(&key) {
            return CacheResult::Miss;
        }
        for entry in self.entries[&key].iter() {
            match entry.outputs(file_store) {
                None => {
                    // TODO: remove the entry because it's not valid anymore
                    unimplemented!(
                        "the flush is not implemented yet, so this code should not be reachable"
                    );
                }
                Some(outputs) => {
                    if entry.is_compatible(execution) {
                        let (exit_status, signal) = match entry.result.status {
                            ExecutionStatus::ReturnCode(c) => (c, None),
                            ExecutionStatus::Signal(s, _) => (0, Some(s)),
                            _ => (0, None),
                        };
                        return CacheResult::Hit {
                            result: ExecutionResult {
                                status: execution.status(
                                    exit_status,
                                    signal,
                                    &entry.result.resources,
                                ),
                                resources: entry.result.resources.clone(),
                            },
                            outputs,
                        };
                    }
                }
            }
        }
        CacheResult::Miss
    }

    /// Checks whether a result is allowed in the cache.
    pub fn is_cacheable(result: &ExecutionResult) -> bool {
        if let ExecutionStatus::InternalError(_) = result.status {
            false
        } else {
            true
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        let mut file = match std::fs::File::create(&self.cache_file) {
            Ok(file) => file,
            Err(e) => {
                error!("Cannot save cache file to disk! {:?}", e);
                return;
            }
        };
        let serialized = match serde_json::to_string(&self.entries.iter().collect_vec()) {
            Ok(data) => data,
            Err(e) => {
                error!("Cannot serialize cache! {:?}", e);
                return;
            }
        };
        match file.write_all(serialized.as_bytes()) {
            Ok(_) => {}
            Err(e) => error!("Cannot write cache file to disk! {:?}", e),
        }
    }
}
