use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use boxfnonce::BoxFnOnce;
use failure::Error;
use serde::{Deserialize, Serialize};

use task_maker_store::*;

use crate::file::*;
use crate::*;

/// The setting of the cache level.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Whether to copy the executables of the compilation inside their default destinations.
    pub copy_exe: bool,
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
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionDAGData {
    /// All the files provided by the client.
    pub provided_files: HashMap<FileUuid, ProvidedFile>,
    /// All the executions to run.
    pub executions: HashMap<ExecutionUuid, Execution>,
    /// The configuration of this DAG.
    pub config: ExecutionDAGConfig,
}

/// A computation DAG, this is not serializable because it contains the callbacks of the client.
#[derive(Debug)]
pub struct ExecutionDAG {
    /// Serializable part of the DAG with all the executions and files.
    pub data: ExecutionDAGData,
    /// Actual callbacks of the executions.
    pub execution_callbacks: HashMap<ExecutionUuid, ExecutionCallbacks>,
    /// Actual callbacks of the files.
    pub file_callbacks: HashMap<FileUuid, FileCallbacks>,
}

impl ExecutionDAG {
    /// Create an empty ExecutionDAG, without files and executions.
    pub fn new() -> ExecutionDAG {
        ExecutionDAG {
            data: ExecutionDAGData {
                provided_files: HashMap::new(),
                executions: HashMap::new(),
                config: ExecutionDAGConfig::new(),
            },
            execution_callbacks: HashMap::new(),
            file_callbacks: HashMap::new(),
        }
    }

    /// Provide a file for the computation.
    pub fn provide_file<P: Into<PathBuf>>(&mut self, file: File, path: P) -> Result<(), Error> {
        let path = path.into();
        self.data.provided_files.insert(
            file.uuid,
            ProvidedFile::LocalFile {
                file,
                key: FileStoreKey::from_file(&path)?,
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
        self.data.executions.insert(execution.uuid, execution);
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
    pub fn get_file_content<G: Into<FileUuid>, F>(&mut self, file: G, limit: usize, callback: F)
    where
        F: (FnOnce(Vec<u8>) -> Result<(), Error>) + 'static,
    {
        self.file_callback(file.into()).get_content = Some((limit, BoxFnOnce::from(callback)));
    }

    /// Add a callback that will be called when the execution starts.
    pub fn on_execution_start<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(WorkerUuid) -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_start
            .push(BoxFnOnce::from(callback));
    }

    /// Add a callback that will be called when the execution ends.
    pub fn on_execution_done<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce(ExecutionResult) -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_done
            .push(BoxFnOnce::from(callback));
    }

    /// Add a callback that will be called when the execution is skipped.
    pub fn on_execution_skip<F>(&mut self, execution: &ExecutionUuid, callback: F)
    where
        F: (FnOnce() -> Result<(), Error>) + 'static,
    {
        self.execution_callback(execution)
            .on_skip
            .push(BoxFnOnce::from(callback));
    }

    /// Get a mutable reference to the config of this DAG.
    pub fn config_mut(&mut self) -> &mut ExecutionDAGConfig {
        &mut self.data.config
    }

    /// Makes sure that a callback item exists for that file and returns a &mut to it.
    fn file_callback<F: Into<FileUuid>>(&mut self, file: F) -> &mut FileCallbacks {
        self.file_callbacks.entry(file.into()).or_default()
    }

    /// Makes sure that a callback item exists for that execution and returns a &mut to it.
    fn execution_callback(&mut self, execution: &ExecutionUuid) -> &mut ExecutionCallbacks {
        self.execution_callbacks.entry(*execution).or_default()
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
            copy_exe: false,
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

    /// Set whether to copy the executables of the compilation inside their default destinations.
    pub fn copy_exe(&mut self, copy_exe: bool) -> &mut Self {
        self.copy_exe = copy_exe;
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

impl From<Option<Option<String>>> for CacheMode {
    fn from(conf: Option<Option<String>>) -> Self {
        match conf {
            None => CacheMode::Everything,
            Some(None) => CacheMode::Nothing,
            Some(Some(list)) => {
                CacheMode::Except(list.split(',').map(ExecutionTag::from).collect())
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
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
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
        assert!(dag.provide_file(file.clone(), "/nope").is_err());
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
        dag.add_execution(exec.clone());
        assert_eq!("exec", &dag.data.executions[&exec.uuid].description);
        assert_eq!(&42.0, &dag.data.executions[&exec.uuid].config.extra_time);
    }

    #[test]
    fn test_write_file_to() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to(file.clone(), "foo", false);
        let write_to = dag.file_callbacks[&file.uuid].write_to.as_ref().unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert_eq!(false, write_to.allow_failure);
        assert_eq!(false, write_to.executable);
    }

    #[test]
    fn test_write_file_to_executable() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to(file.clone(), "foo", true);
        let write_to = dag.file_callbacks[&file.uuid].write_to.as_ref().unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert_eq!(false, write_to.allow_failure);
        assert_eq!(true, write_to.executable);
    }

    #[test]
    fn test_write_file_to_allow_fail() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to_allow_fail(file.clone(), "foo", false);
        let write_to = dag.file_callbacks[&file.uuid].write_to.as_ref().unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert_eq!(true, write_to.allow_failure);
        assert_eq!(false, write_to.executable);
    }

    #[test]
    fn test_write_file_to_allow_fail_executable() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.write_file_to_allow_fail(file.clone(), "foo", true);
        let write_to = dag.file_callbacks[&file.uuid].write_to.as_ref().unwrap();
        assert_eq!(Path::new("foo"), write_to.dest);
        assert_eq!(true, write_to.allow_failure);
        assert_eq!(true, write_to.executable);
    }

    #[test]
    fn test_get_file_content() {
        let mut dag = ExecutionDAG::new();
        let file = File::new("file");
        dag.get_file_content(file.clone(), 1234, |_| Ok(()));
        let (limit, _) = dag.file_callbacks[&file.uuid].get_content.as_ref().unwrap();
        assert_eq!(&1234, limit);
    }

    #[test]
    fn test_on_execution_start() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_start(&exec.uuid, |_| Ok(()));
        assert_eq!(1, dag.execution_callbacks[&exec.uuid].on_start.len());
    }

    #[test]
    fn test_on_execution_done() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_done(&exec.uuid, |_| Ok(()));
        assert_eq!(1, dag.execution_callbacks[&exec.uuid].on_done.len());
    }

    #[test]
    fn test_on_execution_skip() {
        let mut dag = ExecutionDAG::new();
        let exec = Execution::new("exec", ExecutionCommand::local("foo"));
        dag.on_execution_skip(&exec.uuid, || Ok(()));
        assert_eq!(1, dag.execution_callbacks[&exec.uuid].on_skip.len());
    }

    #[test]
    fn test_config_mut() {
        let mut dag = ExecutionDAG::new();
        dag.config_mut().extra_time(123.0);
        assert_abs_diff_eq!(123.0, dag.data.config.extra_time);
    }
}
