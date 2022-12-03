use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Context, Error};
use serde::{Deserialize, Serialize};

use task_maker_store::*;

use crate::file::*;
use crate::*;

/// The setting of the cache level.
#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum CacheMode {
    /// Use the cache as much as possible.
    Everything,
    /// Never use the cache.
    Nothing,
    /// Use the cache except for these tags.
    Except(HashSet<ExecutionTag>),
}

/// Configuration setting of an `ExecutionDAG`, some of the values set here will be inherited in the
/// configuration of the executions added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionDAGConfig {
    /// Keep the sandbox directory of each execution.
    pub keep_sandboxes: bool,
    /// Do not write any file inside the task directory. This works by inhibiting the calls to
    /// `write_file_to`, for this reason only the files added _after_ setting this value to `true`
    /// will be discarded.
    pub dry_run: bool,
    /// The cache mode for this DAG.
    pub cache_mode: CacheMode,
    /// Extra time to give to the sandbox before killing the process, in seconds.
    pub extra_time: f64,
    /// Extra memory to give to the sandbox before killing the process, in KiB.
    pub extra_memory: u64,
    /// Whether to copy the executables of the compilation inside their default destinations.
    pub copy_exe: bool,
    /// Whether to copy the log files of some interesting executions.
    pub copy_logs: bool,
    /// Priority of this DAG.
    pub priority: DagPriority,
}

/// A wrapper around a `File` provided by the client, this means that the client knows the
/// `FileStoreKey` and the path to that file if it's local, or it's content if it's generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProvidedFile {
    /// A file that is provided by the disk, knowing its path.
    LocalFile {
        /// The file handle.
        file: File,
        /// The key of the file for the lookup in the `FileStore`.
        key: FileStoreKey,
        /// Path to the file in the client.
        local_path: PathBuf,
    },
    /// A file that is provided from a in-memory buffer.
    Content {
        /// The file handle.
        file: File,
        /// The key of the file for the lookup in the `FileStore`.
        key: FileStoreKey,
        /// The content of the file.
        content: Vec<u8>,
    },
}

/// Serializable part of the execution DAG: everything except the callbacks (which are not
/// serializable).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExecutionDAGData {
    /// All the files provided by the client.
    pub provided_files: HashMap<FileUuid, ProvidedFile>,
    /// All the executions to run.
    pub execution_groups: HashMap<ExecutionGroupUuid, ExecutionGroup>,
    /// The configuration of this DAG.
    pub config: ExecutionDAGConfig,
}

/// The set of callbacks of a DAG.
#[derive(Debug)]
pub struct ExecutionDAGCallbacks {
    /// The callbacks of the executions.
    pub execution_callbacks: HashMap<ExecutionUuid, ExecutionCallbacks>,
    /// The callbacks of the files.
    pub file_callbacks: HashMap<FileUuid, FileCallbacks>,
    /// Set of the handles of the files that should be sent to the client as soon as possible. The
    /// others will be sent at the end of the evaluation. Note that sending big files during the
    /// evaluation can cause performance degradations.
    pub urgent_files: HashSet<FileUuid>,
}

/// A computation DAG, this is not serializable because it contains the callbacks of the client.
#[derive(Debug)]
pub struct ExecutionDAG {
    /// Serializable part of the DAG with all the executions and files.
    pub data: ExecutionDAGData,
    /// The list of callbacks for the items of the DAG.
    ///
    /// Upon cloning this DAG, the callbacks won't be available anymore. This is an Option to check
    /// that the callbacks are never accessed on the clones.
    pub callbacks: Option<ExecutionDAGCallbacks>,
}

impl ExecutionDAG {
    /// Create an empty ExecutionDAG, without files and executions.
    pub fn new() -> ExecutionDAG {
        ExecutionDAG {
            data: ExecutionDAGData {
                provided_files: HashMap::new(),
                execution_groups: HashMap::new(),
                config: ExecutionDAGConfig::new(),
            },
            callbacks: Some(ExecutionDAGCallbacks {
                execution_callbacks: HashMap::new(),
                file_callbacks: HashMap::new(),
                urgent_files: HashSet::new(),
            }),
        }
    }

    /// Provide a file for the computation.
    pub fn provide_file<P: Into<PathBuf>>(&mut self, file: File, path: P) -> Result<(), Error> {
        let path = path.into();
        self.data.provided_files.insert(
            file.uuid,
            ProvidedFile::LocalFile {
                file,
                key: FileStoreKey::from_file(&path)
                    .with_context(|| format!("Failed to compute file key of {}", path.display()))?,
                local_path: path,
            },
        );
        Ok(())
    }

    /// Provide the content of a file for the computation.
    pub fn provide_content(&mut self, file: File, content: Vec<u8>) {
        self.data.provided_files.insert(
            file.uuid,
            ProvidedFile::Content {
                file,
                key: FileStoreKey::from_content(&content),
                content,
            },
        );
    }

    /// Add an execution to the DAG.
    pub fn add_execution(&mut self, mut execution: Execution) {
        execution.config = self.data.config.clone();
        let mut group = ExecutionGroup::new(execution.description.clone());
        group.add_execution(execution);
        self.data.execution_groups.insert(group.uuid, group);
    }

    /// Add an execution group to the DAG.
    pub fn add_execution_group(&mut self, mut group: ExecutionGroup) {
        for exec in group.executions.iter_mut() {
            exec.config = self.data.config.clone();
        }
        self.data.execution_groups.insert(group.uuid, group);
    }

    /// When `file` is ready it will be written to `path`. The file must be present in the dag
    /// before the evaluation starts.
    ///
    /// If the config `dry_run` is set to true the calls to this function are no-op.
    ///
    /// If the generation of the file fails (i.e. the `Execution` that produced that file was
    /// unsuccessful) the file is **not** written.
    pub fn write_file_to<F: Into<FileUuid>, P: Into<PathBuf>>(
        &mut self,
        file: F,
        path: P,
        executable: bool,
    ) {
        if !self.data.config.dry_run {
            self.file_callback(file.into()).write_to = Some(WriteToCallback {
                dest: path.into(),
                executable,
                allow_failure: false,
            });
        }
    }

    /// Same as `write_file_to` but allowing failures.
    pub fn write_file_to_allow_fail<F: Into<FileUuid>, P: Into<PathBuf>>(
        &mut self,
        file: F,
        path: P,
        executable: bool,
    ) {
        if !self.data.config.dry_run {
            self.file_callback(file.into()).write_to = Some(WriteToCallback {
                dest: path.into(),
                executable,
                allow_failure: true,
            });
        }
    }

    /// Call `callback` with the first `limit` bytes of the file when it's ready. The file must be
    /// present in the DAG before the evaluation starts.
    ///
    /// If the generation of the file fails (i.e. the `Execution` that produced that file was
    /// unsuccessful) the callback **is called** anyways with the content of the file, if any.
    ///
    /// Calling this method twice on the same file will panic (this behaviour can be changed in the
    /// future).
    pub fn get_file_content<G: Into<FileUuid>, F>(&mut self, file: G, limit: usize, callback: F)
    where
        F: (FnOnce(Vec<u8>) -> Result<(), Error>) + 'static,
    {
        let file = file.into();
        // FIXME: add support for multiple get_content file callbacks on the same file. This may be
        // needed for the sanity checks.
        let old = self
            .file_callback(file)
            .get_content
            .replace((limit, Box::new(callback)));
        if old.is_some() {
            panic!("Overwriting get_file_content callback for {}", file);
        }
    }

    /// Call `callback` with the chunks of the file when it's ready. The file must be present in
    /// the DAG before the evaluation starts.
    ///
    /// If the generation of the file fails (i.e. the `Execution` that produced that file was
    /// unsuccessful) the callback **is called** anyways with the content of the file, if any.
    pub fn get_file_content_chunked<G: Into<FileUuid>, F>(&mut self, file: G, callback: F)
    where
        F: (FnMut(&[u8]) -> Result<(), Error>) + 'static,
    {
        let file = file.into();
        self.file_callback(file)
            .get_content_chunked
            .push(Box::new(callback));
    }

    /// Add a callback that will be called when the execution starts.
    pub fn on_execution_start<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(WorkerUuid) -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_start
            .push(Box::new(callback));
    }

    /// Add a callback that will be called when the execution ends.
    pub fn on_execution_done<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(ExecutionResult) -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_done
            .push(Box::new(callback));
    }

    /// Add a callback that will be called when the execution is skipped.
    pub fn on_execution_skip<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce() -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_skip
            .push(Box::new(callback));
    }

    /// Get a mutable reference to the config of this DAG.
    pub fn config_mut(&mut self) -> &mut ExecutionDAGConfig {
        &mut self.data.config
    }

    /// Makes sure that a callback item exists for that file and returns a &mut to it.
    fn file_callback<F: Into<FileUuid>>(&mut self, file: F) -> &mut FileCallbacks {
        self.callbacks
            .as_mut()
            .expect("Cannot change callbacks after cloning")
            .file_callbacks
            .entry(file.into())
            .or_default()
    }

    /// Get the list of registered file callbacks.
    pub fn file_callbacks(&mut self) -> &mut HashMap<FileUuid, FileCallbacks> {
        &mut self.callbacks.as_mut().unwrap().file_callbacks
    }

    /// Makes sure that a callback item exists for that execution and returns a &mut to it.
    fn execution_callback(&mut self, execution: &ExecutionUuid) -> &mut ExecutionCallbacks {
        self.callbacks
            .as_mut()
            .expect("Cannot change callbacks after cloning")
            .execution_callbacks
            .entry(*execution)
            .or_default()
    }

    /// Get the list of registered execution callbacks.
    pub fn execution_callbacks(&mut self) -> &mut HashMap<ExecutionUuid, ExecutionCallbacks> {
        &mut self.callbacks.as_mut().unwrap().execution_callbacks
    }

    /// Mark a file as urgent. The server will try to send it as soon as possible.
    pub fn urgent_file<F: Into<FileUuid>>(&mut self, file: F) {
        self.callbacks
            .as_mut()
            .expect("Cannot change callbacks after cloning")
            .urgent_files
            .insert(file.into());
    }

    /// Get the list of urgent files.
    pub fn urgent_files(&mut self) -> &mut HashSet<FileUuid> {
        &mut self.callbacks.as_mut().unwrap().urgent_files
    }
}

impl Clone for ExecutionDAG {
    /// Clone this `ExecutionDAG`. The callbacks are not cloned, and trying to access them will
    /// result in a panic.
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            callbacks: None,
        }
    }
}

impl ExecutionDAGConfig {
    /// Make a new `ExecutionDAGConfig`.
    pub fn new() -> ExecutionDAGConfig {
        ExecutionDAGConfig {
            keep_sandboxes: false,
            dry_run: false,
            cache_mode: CacheMode::Everything,
            extra_time: 0.5,
            extra_memory: 8 * 1024, // 8 MiB
            copy_exe: false,
            copy_logs: false,
            priority: 0,
        }
    }

    /// Whether to keep the sandbox directory of each execution.
    pub fn keep_sandboxes(&mut self, keep_sandboxes: bool) -> &mut Self {
        self.keep_sandboxes = keep_sandboxes;
        self
    }

    /// Whether to ignore all the subsequent calls to `write_file_to`.
    pub fn dry_run(&mut self, dry_run: bool) -> &mut Self {
        self.dry_run = dry_run;
        self
    }

    /// Set the cache mode for the executions of this DAG.
    pub fn cache_mode(&mut self, cache_mode: CacheMode) -> &mut Self {
        self.cache_mode = cache_mode;
        self
    }

    /// Set the extra time to give to the executions before being killed by the sandbox.
    pub fn extra_time(&mut self, extra_time: f64) -> &mut Self {
        assert!(extra_time >= 0.0);
        self.extra_time = extra_time;
        self
    }

    /// Set the extra memory to give to the executions before being killed by the sandbox.
    pub fn extra_memory(&mut self, extra_memory: u64) -> &mut Self {
        self.extra_memory = extra_memory;
        self
    }

    /// Set whether to copy the executables of the compilation inside their default destinations.
    pub fn copy_exe(&mut self, copy_exe: bool) -> &mut Self {
        self.copy_exe = copy_exe;
        self
    }

    /// Set whether to copy the log files of some interesting executions.
    pub fn copy_logs(&mut self, copy_logs: bool) -> &mut Self {
        self.copy_logs = copy_logs;
        self
    }

    /// Set the priority of this DAG.
    pub fn priority(&mut self, priority: DagPriority) -> &mut Self {
        self.priority = priority;
        self
    }
}

impl Default for ExecutionDAGConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ExecutionDAG {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheMode {
    /// Make a `CacheMode` from the command line arguments:
    /// * `None`: cache enabled
    /// * `Some(None)`: cache disabled
    /// * `Some(Some(comma separated list))`: cache disabled for the specified tags
    #[allow(clippy::option_option)]
    pub fn try_from(
        conf: &Option<Option<String>>,
        valid_tags: &[String],
    ) -> Result<CacheMode, Error> {
        match conf {
            None => Ok(CacheMode::Everything),
            Some(None) => Ok(CacheMode::Nothing),
            Some(Some(list)) => {
                let tags: HashSet<_> = list.split(',').map(ExecutionTag::from).collect();
                for tag in tags.iter() {
                    if !valid_tags.contains(&tag.name) {
                        bail!(
                            "Invalid cache mode: {} (valid are: {})",
                            tag.name,
                            valid_tags.join(", ")
                        );
                    }
                }
                Ok(CacheMode::Except(tags))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_provide_file() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let file_path = tmpdir.path().join("foo");
        std::fs::write(&file_path, "bar").unwrap();
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.provide_file(file.clone(), &file_path).unwrap();
        match &dag.data.provided_files[&file.uuid] {
            ProvidedFile::LocalFile {
                file, local_path, ..
            } => {
                assert_eq!("file", &file.description);
                assert_eq!(&file_path, local_path);
            }
            _ => panic!("Invalid provided file type"),
        }
    }

    #[test]
    fn test_provide_file_not_existing() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        assert!(dag.provide_file(file, "/nope").is_err());
    }

    #[test]
    fn test_provide_content() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.provide_content(file.clone(), b"ciao".to_vec());
        match &dag.data.provided_files[&file.uuid] {
            ProvidedFile::Content { file, content, .. } => {
                assert_eq!("file", &file.description);
                assert_eq!(b"ciao", content.as_slice());
            }
            _ => panic!("Invalid provided file type"),
        }
    }

    #[test]
    fn test_add_execution() {
        let mut dag = ExecutionDAG::new();
        dag.config_mut().extra_time(42.0);
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.add_execution(exec);
        let group_uuid = dag.data.execution_groups.keys().next().unwrap();
        assert_eq!("exec", &dag.data.execution_groups[group_uuid].description);
        assert_abs_diff_eq!(
            &42.0,
            &dag.data.execution_groups[group_uuid].config().extra_time
        );
    }

    #[test]
    fn test_write_file_to() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to(file.clone(), "foo", false);
        let write_to = dag.callbacks.as_mut().unwrap().file_callbacks[&file.uuid]
            .write_to
            .as_ref()
            .unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert!(!write_to.allow_failure);
        assert!(!write_to.executable);
    }

    #[test]
    fn test_write_file_to_executable() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to(file.clone(), "foo", true);
        let write_to = dag.callbacks.as_mut().unwrap().file_callbacks[&file.uuid]
            .write_to
            .as_ref()
            .unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert!(!write_to.allow_failure);
        assert!(write_to.executable);
    }

    #[test]
    fn test_write_file_to_allow_fail() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to_allow_fail(file.clone(), "foo", false);
        let write_to = dag.callbacks.as_mut().unwrap().file_callbacks[&file.uuid]
            .write_to
            .as_ref()
            .unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert!(write_to.allow_failure);
        assert!(!write_to.executable);
    }

    #[test]
    fn test_write_file_to_allow_fail_executable() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to_allow_fail(file.clone(), "foo", true);
        let write_to = dag.callbacks.as_mut().unwrap().file_callbacks[&file.uuid]
            .write_to
            .as_ref()
            .unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert!(write_to.allow_failure);
        assert!(write_to.executable);
    }

    #[test]
    fn test_get_file_content() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.get_file_content(file.clone(), 1234, |_| Ok(()));
        let (limit, _) = dag.callbacks.as_mut().unwrap().file_callbacks[&file.uuid]
            .get_content
            .as_ref()
            .unwrap();
        assert_eq!(&1234, limit);
    }

    #[test]
    fn test_on_execution_start() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_start(&exec.uuid, |_| Ok(()));
        assert_eq!(
            1,
            dag.callbacks.unwrap().execution_callbacks[&exec.uuid]
                .on_start
                .len()
        );
    }

    #[test]
    fn test_on_execution_done() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_done(&exec.uuid, |_| Ok(()));
        assert_eq!(
            1,
            dag.callbacks.unwrap().execution_callbacks[&exec.uuid]
                .on_done
                .len()
        );
    }

    #[test]
    fn test_on_execution_skip() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_skip(&exec.uuid, || Ok(()));
        assert_eq!(
            1,
            dag.callbacks.unwrap().execution_callbacks[&exec.uuid]
                .on_skip
                .len()
        );
    }

    #[test]
    fn test_config_mut() {
        let mut dag = ExecutionDAG::new();
        dag.config_mut().extra_time(123.0);
        assert_abs_diff_eq!(123.0, dag.data.config.extra_time);
    }

    #[test]
    fn test_urgent_files() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file".to_string());
        dag.urgent_file(&file);
        assert!(dag.callbacks.unwrap().urgent_files.contains(&file.uuid));
    }

    #[test]
    fn test_cache_mode_try_from() {
        assert_eq!(
            CacheMode::try_from(&None, &[]).unwrap(),
            CacheMode::Everything
        );
        assert_eq!(
            CacheMode::try_from(&Some(None), &[]).unwrap(),
            CacheMode::Nothing
        );
        assert_eq!(
            CacheMode::try_from(&Some(Some("tag1".to_string())), &["tag1".to_string()]).unwrap(),
            CacheMode::Except(vec![ExecutionTag::from("tag1")].into_iter().collect())
        );
        assert!(CacheMode::try_from(&Some(Some("tag1".to_string())), &[]).is_err());
    }
}
