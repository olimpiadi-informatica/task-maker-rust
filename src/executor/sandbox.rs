use crate::execution::*;
use crate::store::*;
use failure::Error;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempdir::TempDir;

/// Result of the execution of the sandbox
#[derive(Debug)]
pub enum SandboxResult {
    /// The sandbox exited succesfully, the statistics about the sandboxed
    /// process are reported
    Success {
        /// The exit status of the process
        exit_status: u32,
        /// The signal that caused the process to exit
        signal: Option<u32>,
        /// Resources used by the process
        resources: ExecutionResourcesUsage,
    },
    /// The sandbox failed to execute the process, an error message is reported
    Failed {
        /// The error reported by the sandbox
        error: String,
    },
}

/// Internals of the sandbox
#[derive(Debug)]
struct SandboxData {
    /// Handle to the temporary directory, will be deleted on drop
    boxdir: TempDir,
}

/// Wrapper around the sandbox. Cloning this struct will keep the reference of
/// the same sandbox, keeping the content alive.
///
/// This sandbox works only on Unix systems because it needs to set the
/// executable bit on some files.
#[derive(Debug, Clone)]
pub struct Sandbox {
    /// Internal data of the sandbox
    data: Arc<Mutex<SandboxData>>,
}

impl Sandbox {
    /// Make a new sandbox for the specified execution, copying all the
    /// required files. To start the sandbox call `run`.
    pub fn new(
        sandboxes_dir: &Path,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreKey>,
        file_store: &mut FileStore,
    ) -> Result<Sandbox, Error> {
        std::fs::create_dir_all(sandboxes_dir)?;
        let boxdir = TempDir::new_in(sandboxes_dir, "box")?;
        Sandbox::setup(boxdir.path(), execution, dep_keys, file_store)?;
        Ok(Sandbox {
            data: Arc::new(Mutex::new(SandboxData { boxdir: boxdir })),
        })
    }

    /// Starts the sandbox and blocks the thread until the sandbox exists.
    pub fn run(&self) -> Result<SandboxResult, Error> {
        trace!(
            "Running sandbox at {:?}",
            self.data.lock().unwrap().boxdir.path()
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
        trace!(
            "Sandbox at {:?} completed",
            self.data.lock().unwrap().boxdir.path()
        );
        Ok(SandboxResult::Success {
            exit_status: 0,
            signal: None,
            resources: ExecutionResourcesUsage {
                cpu_time: 42.0,
                sys_time: 43.0,
                wall_time: 90.0,
                memory: 1234,
            },
        })
    }

    /// Tell the sandbox process to kill the underlying process, this will make
    /// `run` terminate more quickly.
    pub fn kill(&self) {
        info!(
            "Sandbox at {:?} got killed",
            self.data.lock().unwrap().boxdir.path()
        );
        unimplemented!();
    }

    /// Make the sandbox persistent, the sandbox directory won't be deleted
    /// after the execution.
    pub fn keep(&self) {
        unimplemented!();
    }

    /// Path of the file where the standard output is written to
    pub fn stdout_path(&self) -> PathBuf {
        self.data.lock().unwrap().boxdir.path().join("stdout")
    }

    /// Path of the file where the standard error is written to
    pub fn stderr_path(&self) -> PathBuf {
        self.data.lock().unwrap().boxdir.path().join("stderr")
    }

    /// Path of the file where that output file is written to
    pub fn output_path(&self, output: &Path) -> PathBuf {
        self.data
            .lock()
            .unwrap()
            .boxdir
            .path()
            .join("box")
            .join(output)
    }

    /// Setup the sandbox directory with all the files required for the
    /// execution
    fn setup(
        dir: &Path,
        execution: &Execution,
        dep_keys: &HashMap<FileUuid, FileStoreKey>,
        file_store: &mut FileStore,
    ) -> Result<(), Error> {
        trace!(
            "Setting up sandbox at {:?} for '{}'",
            dir,
            execution.description
        );
        if let Some(stdin) = execution.stdin {
            Sandbox::write_sandbox_file(
                &dir.join("stdin"),
                dep_keys.get(&stdin).expect("stdin not provided"),
                false,
                file_store,
            )?;
        }
        if execution.stdout.is_some() {
            Sandbox::touch_file(&dir.join("stdout"), 0o600)?;
        }
        if execution.stderr.is_some() {
            Sandbox::touch_file(&dir.join("stderr"), 0o600)?;
        }
        for input in execution.inputs.iter() {
            Sandbox::write_sandbox_file(
                &dir.join("box").join(&input.path),
                dep_keys.get(&input.file).expect("file not provided"),
                input.executable,
                file_store,
            )?;
        }
        for path in execution.outputs.keys() {
            Sandbox::touch_file(&dir.join("box").join(&path), 0o600)?;
        }
        trace!("Sandbox at {:?} ready!", dir);
        Ok(())
    }

    /// Put a file inside the sandbox, creating the directories if needed and
    /// making it executable if needed.
    fn write_sandbox_file(
        dest: &Path,
        key: &FileStoreKey,
        executable: bool,
        file_store: &mut FileStore,
    ) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        let path = file_store.get(key)?.expect("file not present in store");
        std::fs::copy(&path, dest)?;
        let mut permisions = std::fs::metadata(&dest)?.permissions();
        if executable {
            permisions.set_mode(0o500);
        } else {
            permisions.set_mode(0o400);
        }
        std::fs::set_permissions(dest, permisions)?;
        Ok(())
    }

    /// Create an empty file inside the sandbox and chmod-it.
    fn touch_file(dest: &Path, mode: u32) -> Result<(), Error> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::File::create(dest)?;
        let mut permisions = std::fs::metadata(&dest)?.permissions();
        permisions.set_mode(mode);
        std::fs::set_permissions(dest, permisions)?;
        Ok(())
    }
}
