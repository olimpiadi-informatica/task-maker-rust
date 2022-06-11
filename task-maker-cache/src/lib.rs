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
//! use task_maker_store::{FileStore, FileStoreKey, ReadFileIterator};
//!
//! // make a new store and a new cache in a testing environment
//! let dir = TempDir::new("tm-test").unwrap();
//! let mut cache = Cache::new(dir.path()).expect("Cannot create the cache");
//! let mut store = FileStore::new(dir.path(), 1000, 1000).expect("Cannot create the store");
//!
//! // setup a testing file
//! let path = dir.path().join("file.txt");
//! std::fs::write(&path, [1, 2, 3, 4]).unwrap();
//!
//! // build a testing execution
//! let mut exec = Execution::new("Testing exec", ExecutionCommand::system("true"));
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
//!     },
//!     was_killed: false,
//!     was_cached: false,
//!     stderr: None,
//!     stdout: None,
//! };
//!
//! // make the FileUuid -> FileStoreHandle map
//! let key = FileStoreKey::from_file(&path).unwrap();
//! let mut file_keys = HashMap::new();
//! file_keys.insert(input.uuid, store.store(&key, ReadFileIterator::new(&path).unwrap()).unwrap());
//!
//! // insert the result in the cache
//! cache.insert(&exec.clone().into(), &file_keys, vec![result]);
//!
//! // retrieve the result from the cache
//! let res = cache.get(&exec.into(), &file_keys, &mut store);
//! match res {
//!     CacheResult::Miss => panic!("Expecting a hit"),
//!     CacheResult::Hit { result, outputs } => {
//!         assert_eq!(result[0].status, ExecutionStatus::Success);
//!         assert_eq!(result[0].resources.memory, 12345);
//!     }
//! }
//! ```

#![deny(missing_docs)]
#![allow(clippy::upper_case_acronyms)]

#[macro_use]
extern crate log;

mod entry;
mod key;
mod storage;
use entry::CacheEntry;
use key::CacheKey;
use storage::CacheFile;

use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::HashMap;
use std::fs::create_dir_all;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use anyhow::{Context, Error};
use itertools::Itertools;

use task_maker_dag::{ExecutionGroup, ExecutionResult, ExecutionStatus, FileUuid};
use task_maker_store::{FileStore, FileStoreHandle};

/// The name of the file which holds the cache data.
const CACHE_FILE: &str = "cache.bin";

/// Handle the cached executions, loading and storing them to disk.
#[derive(Debug)]
pub struct Cache {
    /// Entries to flush to disk.
    to_flush: HashMap<u8, CacheFile>,
    /// The base directory where the cache files are stored.
    cache_dir: PathBuf,
}

/// The result of a cache query, can be either successful (`Hit`) or unsuccessful (`Miss`).
pub enum CacheResult {
    /// The requested entry is not present in the cache.
    Miss,
    /// The requested entry is present in the cache.
    Hit {
        /// The result of the execution.
        result: Vec<ExecutionResult>,
        /// The outputs of the execution.
        outputs: HashMap<FileUuid, FileStoreHandle>,
    },
}

impl Cache {
    /// Make a new `Cache` stored in the specified cache directory. Returns an error if the cache
    /// directory cannot be created.
    pub fn new<P: Into<PathBuf>>(cache_dir: P) -> Result<Cache, Error> {
        let cache_dir = cache_dir.into();
        create_dir_all(&cache_dir).with_context(|| {
            format!("Failed to create cache directory: {}", cache_dir.display())
        })?;
        Ok(Self {
            to_flush: Default::default(),
            cache_dir,
        })
    }

    /// Insert a new entry inside the cache. They key is computed based on the execution's metadata
    /// and on the hash of it's inputs, defined by the mapping `file_keys` from the UUIDs of the DAG
    /// to the persistent `FileStoreKey`s.
    pub fn insert(
        &mut self,
        group: &ExecutionGroup,
        file_keys: &HashMap<FileUuid, FileStoreHandle>,
        result: Vec<ExecutionResult>,
    ) {
        let key = CacheKey::from_execution_group(group, file_keys);
        let file = match self.get_file(&key) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to insert entry in the cache: {:?}", e);
                return;
            }
        };

        let set = file.entry(key).or_default();
        let entry = CacheEntry::from_execution_group(group, file_keys, result);
        // Do not insert duplicated keys, replace if the limits are the same.
        let pos = set.iter().find_position(|e| e.same_limits(&entry));
        if let Some((pos, _)) = pos {
            set[pos] = entry;
        } else {
            set.push(entry);
        }
        file.mark_dirty();
    }

    /// Search in the cache for a valid entry, returning a cache hit if it's found or a cache miss
    /// if not.
    ///
    /// The result contains the handles to the files in the `FileStore`, preventing the flushing
    /// from erasing them.
    pub fn get(
        &mut self,
        group: &ExecutionGroup,
        file_keys: &HashMap<FileUuid, FileStoreHandle>,
        file_store: &FileStore,
    ) -> CacheResult {
        let key = CacheKey::from_execution_group(group, file_keys);
        let file = match self.get_file(&key) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to get entry from the cache: {:?}", e);
                return CacheResult::Miss;
            }
        };
        let entry = file.entry(key);
        let entry = match &entry {
            Entry::Vacant(_) => return CacheResult::Miss,
            Entry::Occupied(entry) => entry.get(),
        };

        for entry in entry.iter() {
            match entry.outputs(file_store, group) {
                None => {
                    // TODO: remove the entry because it's not valid anymore
                }
                Some(outputs) => {
                    if entry.is_compatible(group) {
                        let mut results = Vec::new();
                        for (exec, item) in group.executions.iter().zip(entry.items.iter()) {
                            let (exit_status, signal) = match &item.result.status {
                                ExecutionStatus::ReturnCode(c) => (*c, None),
                                ExecutionStatus::Signal(s, name) => (0, Some((*s, name.clone()))),
                                _ => (0, None),
                            };
                            results.push(ExecutionResult {
                                status: exec.status(exit_status, signal, &item.result.resources),
                                was_killed: item.result.was_killed,
                                was_cached: true,
                                resources: item.result.resources.clone(),
                                stdout: item.result.stdout.clone(),
                                stderr: item.result.stderr.clone(),
                            });
                        }
                        return CacheResult::Hit {
                            result: results,
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
        !matches!(result.status, ExecutionStatus::InternalError(_))
    }

    /// Try to load the cache file for this key.
    fn get_file(&mut self, key: &CacheKey) -> Result<&mut CacheFile, Error> {
        let mut hasher = DefaultHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let lv1 = (hash % 256) as u8;
        match self.to_flush.entry(lv1) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let path = self.cache_dir.join(lv1.to_string()).join(CACHE_FILE);
                Ok(entry.insert(CacheFile::load(path)?))
            }
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        for (_, file) in self.to_flush.drain() {
            if let Err(e) = file.store() {
                warn!("Failed to store cache file: {:?}", e);
            }
        }
    }
}
