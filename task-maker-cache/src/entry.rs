use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use task_maker_dag::{Execution, ExecutionLimits, ExecutionResult, ExecutionStatus, FileUuid};
use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey};

/// A cache entry for a given cache key. Note that the result will be used only if:
/// - all the required output files are still valid (ie inside the `FileStore`).
/// - the limits are compatible with the limits of the query.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CacheEntry {
    /// The result of the `Execution`.
    pub result: ExecutionResult,
    /// The limits associated with this entry.
    pub limits: ExecutionLimits,
    /// The key (aka the hash) of the stdout, if any.
    pub stdout: Option<FileStoreKey>,
    /// The key (aka the hash) of the stderr, if any.
    pub stderr: Option<FileStoreKey>,
    /// The key (aka the hash) of the output files, indexed by their path inside the sandbox.
    pub outputs: HashMap<PathBuf, FileStoreKey>,
}

impl CacheEntry {
    /// Search in the file store the handles of all the output files. Will return `None` if at least
    /// one of them is missing.
    pub fn outputs(
        &self,
        file_store: &FileStore,
        exec: &Execution,
    ) -> Option<HashMap<FileUuid, FileStoreHandle>> {
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

        let mut outputs = HashMap::new();

        if let Some(stdout) = exec.stdout.as_ref() {
            if let Some(handle) = try_get!(self.stdout) {
                outputs.insert(stdout.uuid, handle);
            } else {
                return None;
            }
        }
        if let Some(stderr) = exec.stderr.as_ref() {
            if let Some(handle) = try_get!(self.stderr) {
                outputs.insert(stderr.uuid, handle);
            } else {
                return None;
            }
        }
        for (path, file) in exec.outputs.iter() {
            if let Some(handle) = try_get!(self.outputs.get(path)) {
                outputs.insert(file.uuid, handle);
            } else {
                return None;
            }
        }
        Some(outputs)
    }

    /// Checks whether a given execution is compatible with the limits stored in this entry. See the
    /// docs of the crate for the definition of _compatible_.
    pub fn is_compatible(&self, execution: &Execution) -> bool {
        // makes sure that $left <= $right where None = inf
        // if $left is less restrictive than $right, return false
        macro_rules! check_limit {
            ($left:expr, $right:expr) => {
                match ($left, $right) {
                    (Some(left), Some(right)) => {
                        if left > right {
                            return false;
                        }
                    }
                    (None, Some(_)) => return false,
                    _ => {}
                }
            };
        }
        // will return false if $less is less restrictive of $right
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
                if $left.read_only < $right.read_only {
                    return false;
                }
                if $left.mount_tmpfs > $right.mount_tmpfs {
                    return false;
                }
                let left_readable_dirs: HashSet<PathBuf> =
                    $left.extra_readable_dirs.iter().cloned().collect();
                let right_readable_dirs: HashSet<PathBuf> =
                    $right.extra_readable_dirs.iter().cloned().collect();
                if left_readable_dirs != right_readable_dirs
                    && left_readable_dirs.is_superset(&right_readable_dirs)
                {
                    return false;
                }
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

#[cfg(test)]
mod tests {
    use crate::entry::CacheEntry;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use task_maker_dag::{
        Execution, ExecutionCommand, ExecutionResourcesUsage, ExecutionResult, ExecutionStatus,
    };
    use task_maker_store::{FileStore, FileStoreHandle, FileStoreKey, ReadFileIterator};

    fn fake_file<P: AsRef<Path>>(path: P, content: &str, store: &FileStore) -> FileStoreHandle {
        File::create(path.as_ref())
            .unwrap()
            .write_all(&content.as_bytes())
            .unwrap();
        let key = FileStoreKey::from_file(path.as_ref()).unwrap();
        let iter = ReadFileIterator::new(path).unwrap();
        store.store(&key, iter).unwrap()
    }

    fn empty_entry() -> (CacheEntry, Execution) {
        (
            CacheEntry {
                result: ExecutionResult {
                    status: ExecutionStatus::Success,
                    was_killed: false,
                    was_cached: false,
                    resources: ExecutionResourcesUsage {
                        cpu_time: 0.0,
                        sys_time: 0.0,
                        wall_time: 0.0,
                        memory: 0,
                    },
                },
                limits: Default::default(),
                stdout: None,
                stderr: None,
                outputs: Default::default(),
            },
            Execution::new("exec", ExecutionCommand::local("foo")),
        )
    }

    #[test]
    fn test_outputs_empty() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();
        let (entry, exec) = empty_entry();
        assert_eq!(entry.outputs(&store, &exec), Some(HashMap::new()));
    }

    #[test]
    fn test_outputs_stdout() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        let file = exec.stdout();
        let hdl = fake_file(tmpdir.path().join("file"), "file", &store);
        entry.stdout = Some(hdl.key().clone());

        assert_eq!(entry.outputs(&store, &exec).unwrap()[&file.uuid], hdl);
    }

    #[test]
    fn test_outputs_stdout_missing() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        exec.stdout();
        let key = FileStoreKey::from_content(&[1, 2, 3]);
        entry.stdout = Some(key);

        assert_eq!(entry.outputs(&store, &exec), None);
    }

    #[test]
    fn test_outputs_stderr() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        let file = exec.stderr();
        let hdl = fake_file(tmpdir.path().join("file"), "file", &store);
        entry.stderr = Some(hdl.key().clone());

        assert_eq!(entry.outputs(&store, &exec).unwrap()[&file.uuid], hdl);
    }

    #[test]
    fn test_outputs_stderr_missing() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        exec.stderr();
        let key = FileStoreKey::from_content(&[1, 2, 3]);
        entry.stderr = Some(key);

        assert_eq!(entry.outputs(&store, &exec), None);
    }

    #[test]
    fn test_outputs_file() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        let file = exec.output("file");
        let hdl = fake_file(tmpdir.path().join("file"), "file", &store);
        entry
            .outputs
            .insert(PathBuf::from("file"), hdl.key().clone());

        assert_eq!(entry.outputs(&store, &exec).unwrap()[&file.uuid], hdl);
    }

    #[test]
    fn test_outputs_file_missing() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();

        let (mut entry, mut exec) = empty_entry();
        exec.output("file");
        let key = FileStoreKey::from_content(&[1, 2, 3]);
        entry.outputs.insert(PathBuf::from("file"), key);

        assert_eq!(entry.outputs(&store, &exec), None);
    }

    #[test]
    fn test_compatible_success_cpu_time() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.cpu_time = Some(1.0);
        entry.result.status = ExecutionStatus::Success;
        entry.limits.cpu_time = Some(1.0);
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.cpu_time = Some(2.0);
        assert!(entry.is_compatible(&exec2));

        let mut exec3 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec3.limits.cpu_time = None;
        assert!(entry.is_compatible(&exec3));

        let mut exec4 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec4.limits.cpu_time = Some(0.5);
        assert!(!entry.is_compatible(&exec4));
    }

    #[test]
    fn test_compatible_fail_cpu_time() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.cpu_time = Some(1.0);
        entry.result.status = ExecutionStatus::TimeLimitExceeded;
        entry.limits.cpu_time = Some(1.0);
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.cpu_time = Some(2.0);
        assert!(!entry.is_compatible(&exec2));

        let mut exec3 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec3.limits.cpu_time = None;
        assert!(!entry.is_compatible(&exec3));

        let mut exec4 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec4.limits.cpu_time = Some(0.5);
        assert!(entry.is_compatible(&exec4));
    }

    #[test]
    fn test_compatible_success_read_only() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.read_only = true;
        entry.result.status = ExecutionStatus::Success;
        entry.limits.read_only = true;
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.read_only = false;
        assert!(entry.is_compatible(&exec2));
    }

    #[test]
    fn test_compatible_success_not_read_only() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.read_only = false;
        entry.result.status = ExecutionStatus::Success;
        entry.limits.read_only = false;
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.read_only = true;
        assert!(!entry.is_compatible(&exec2));
    }

    #[test]
    fn test_compatible_fail_read_only() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.read_only = true;
        entry.result.status = ExecutionStatus::ReturnCode(1);
        entry.limits.read_only = true;
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.read_only = false;
        assert!(!entry.is_compatible(&exec2));
    }

    #[test]
    fn test_compatible_fail_not_read_only() {
        let (mut entry, mut exec1) = empty_entry();
        exec1.limits.read_only = false;
        entry.result.status = ExecutionStatus::ReturnCode(1);
        entry.limits.read_only = false;
        assert!(entry.is_compatible(&exec1));

        let mut exec2 = Execution::new("exec", ExecutionCommand::local("foo"));
        exec2.limits.read_only = true;
        assert!(entry.is_compatible(&exec2));
    }
}
