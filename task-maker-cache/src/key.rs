use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use task_maker_dag::{Execution, ExecutionCommand, FileUuid};
use task_maker_store::{FileStoreHandle, FileStoreKey};

/// The cache key used to address the cache entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    /// The command of the execution. Note that this assumes that the system commands are all the
    /// same between the different workers.
    pub command: ExecutionCommand,
    /// The list of command line arguments.
    pub args: Vec<String>,
    /// The key (aka the hash) of the stdin, if any.
    pub stdin: Option<FileStoreKey>,
    /// The key (aka the hash) of the input files, and if they are executable. Note that because the
    /// order matters here (it changes the final hash of the key) those values are sorted
    /// lexicographically.
    pub inputs: Vec<(PathBuf, FileStoreKey, bool)>,
    /// The list of environment variables to set. Sorted by the variable name.
    pub env: Vec<(String, String)>,
}

impl CacheKey {
    /// Make a new `CacheKey` based on an `Execution` and on the mapping of its input files, from
    /// the UUIDs of the current DAG to the persisted `FileStoreKey`s.
    pub fn from_execution(
        execution: &Execution,
        file_keys: &HashMap<FileUuid, FileStoreHandle>,
    ) -> CacheKey {
        let stdin = execution.stdin.as_ref().map(|f| file_keys[f].key().clone());
        let inputs = execution
            .inputs
            .clone()
            .into_iter()
            .map(|(p, f)| (p, file_keys[&f.file].key().clone(), f.executable))
            .sorted()
            .collect_vec();
        let env = execution.env.clone().into_iter().sorted().collect_vec();
        CacheKey {
            command: execution.command.clone(),
            args: execution.args.clone(),
            stdin,
            inputs,
            env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::fs::File;
    use std::hash::{Hash, Hasher};
    use std::io::Write;
    use std::path::Path;
    use task_maker_store::{FileStore, ReadFileIterator};

    fn fake_file<P: AsRef<Path>>(path: P, content: &str, store: &FileStore) -> FileStoreHandle {
        File::create(path.as_ref())
            .unwrap()
            .write_all(&content.as_bytes())
            .unwrap();
        let key = FileStoreKey::from_file(path.as_ref()).unwrap();
        let iter = ReadFileIterator::new(path).unwrap();
        store.store(&key, iter).unwrap()
    }

    fn hash(key: &CacheKey) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn test_command() {
        let exec1 = Execution::new("exec1", ExecutionCommand::local("foo"));
        let exec2 = Execution::new("exec2", ExecutionCommand::local("foo"));
        let exec3 = Execution::new("exec3", ExecutionCommand::local("bar"));
        let exec4 = Execution::new("exec4", ExecutionCommand::system("foo"));
        let key1 = CacheKey::from_execution(&exec1, &HashMap::new());
        let key2 = CacheKey::from_execution(&exec2, &HashMap::new());
        let key3 = CacheKey::from_execution(&exec3, &HashMap::new());
        let key4 = CacheKey::from_execution(&exec4, &HashMap::new());
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(hash(&key1), hash(&key2));
        assert_ne!(hash(&key1), hash(&key3));
        assert_ne!(hash(&key1), hash(&key4));
    }

    #[test]
    fn test_args() {
        let mut exec1 = Execution::new("exec1", ExecutionCommand::local("foo"));
        exec1.args(vec!["bar", "baz"]);
        let mut exec2 = Execution::new("exec2", ExecutionCommand::local("foo"));
        exec2.args(vec!["bar", "baz"]);
        let mut exec3 = Execution::new("exec3", ExecutionCommand::local("foo"));
        exec3.args(vec!["baz", "bar"]);
        let mut exec4 = Execution::new("exec4", ExecutionCommand::local("foo"));
        exec4.args(vec!["bar", "bar"]);
        let key1 = CacheKey::from_execution(&exec1, &HashMap::new());
        let key2 = CacheKey::from_execution(&exec2, &HashMap::new());
        let key3 = CacheKey::from_execution(&exec3, &HashMap::new());
        let key4 = CacheKey::from_execution(&exec4, &HashMap::new());
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(hash(&key1), hash(&key2));
        assert_ne!(hash(&key1), hash(&key3));
        assert_ne!(hash(&key1), hash(&key4));
    }

    #[test]
    fn test_stdin() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();
        let handle1 = fake_file(tmpdir.path().join("file1"), "foo", &store);
        let handle2 = fake_file(tmpdir.path().join("file2"), "bar", &store);
        let file1 = task_maker_dag::File::new("file1");
        let file2 = task_maker_dag::File::new("file1");
        let map: HashMap<_, _> = [(file1.uuid, handle1), (file2.uuid, handle2)]
            .iter()
            .cloned()
            .collect();
        let mut exec1 = Execution::new("exec1", ExecutionCommand::local("foo"));
        exec1.stdin(file1.uuid);
        let mut exec2 = Execution::new("exec2", ExecutionCommand::local("foo"));
        exec2.stdin(file1.uuid);
        let mut exec3 = Execution::new("exec3", ExecutionCommand::local("foo"));
        exec3.stdin(file2.uuid);
        let exec4 = Execution::new("exec4", ExecutionCommand::local("foo"));
        let key1 = CacheKey::from_execution(&exec1, &map);
        let key2 = CacheKey::from_execution(&exec2, &map);
        let key3 = CacheKey::from_execution(&exec3, &map);
        let key4 = CacheKey::from_execution(&exec4, &map);
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(hash(&key1), hash(&key2));
        assert_ne!(hash(&key1), hash(&key3));
        assert_ne!(hash(&key1), hash(&key4));
    }

    #[test]
    fn test_inputs() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let store = FileStore::new(tmpdir.path(), 1000, 1000).unwrap();
        let handle1 = fake_file(tmpdir.path().join("file1"), "foo", &store);
        let handle2 = fake_file(tmpdir.path().join("file2"), "bar", &store);
        let file1 = task_maker_dag::File::new("file1");
        let file2 = task_maker_dag::File::new("file1");
        let map: HashMap<_, _> = [(file1.uuid, handle1), (file2.uuid, handle2)]
            .iter()
            .cloned()
            .collect();
        let mut exec1 = Execution::new("exec1", ExecutionCommand::local("foo"));
        exec1.input(file1.uuid, "file1", false);
        exec1.input(file2.uuid, "file2", false);
        let mut exec2 = Execution::new("exec2", ExecutionCommand::local("foo"));
        exec2.input(file2.uuid, "file2", false);
        exec2.input(file1.uuid, "file1", false);
        let mut exec3 = Execution::new("exec3", ExecutionCommand::local("foo"));
        exec3.input(file1.uuid, "file1", false);
        let mut exec4 = Execution::new("exec4", ExecutionCommand::local("foo"));
        exec4.input(file1.uuid, "file1", true);
        exec4.input(file2.uuid, "file2", false);
        let key1 = CacheKey::from_execution(&exec1, &map);
        let key2 = CacheKey::from_execution(&exec2, &map);
        let key3 = CacheKey::from_execution(&exec3, &map);
        let key4 = CacheKey::from_execution(&exec4, &map);
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(hash(&key1), hash(&key2));
        assert_ne!(hash(&key1), hash(&key3));
        assert_ne!(hash(&key1), hash(&key4));
    }

    #[test]
    fn test_env() {
        let mut exec1 = Execution::new("exec1", ExecutionCommand::local("foo"));
        exec1.env("foo", "bar");
        exec1.env("baz", "biz");
        let mut exec2 = Execution::new("exec2", ExecutionCommand::local("foo"));
        exec2.env("baz", "biz");
        exec2.env("foo", "bar");
        let mut exec3 = Execution::new("exec3", ExecutionCommand::local("foo"));
        exec3.env("foo", "bar");
        exec3.env("baz", "bizarre");
        let mut exec4 = Execution::new("exec4", ExecutionCommand::local("foo"));
        exec4.env("foo", "bar");
        let key1 = CacheKey::from_execution(&exec1, &HashMap::new());
        let key2 = CacheKey::from_execution(&exec2, &HashMap::new());
        let key3 = CacheKey::from_execution(&exec3, &HashMap::new());
        let key4 = CacheKey::from_execution(&exec4, &HashMap::new());
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(hash(&key1), hash(&key2));
        assert_ne!(hash(&key1), hash(&key3));
        assert_ne!(hash(&key1), hash(&key4));
    }
}
