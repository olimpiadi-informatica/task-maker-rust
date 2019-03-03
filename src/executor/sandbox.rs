use crate::execution::*;
use crate::store::*;
use failure::Error;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempdir::TempDir;

/// Internals of the sandbox
struct SandboxData {
    /// Handle to the temporary directory, will be deleted on drop
    boxdir: TempDir,
}

/// Wrapper around the sandbox.
///
/// This sandbox works only on Unix systems because it needs to set the
/// executable bit on some files.
pub struct Sandbox {
    /// Internal data of the sandbox
    data: SandboxData,
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
            data: SandboxData { boxdir },
        })
    }

    /// Starts the sandbox and blocks the thread until the sandbox exists.
    pub fn run() {
        unimplemented!();
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
            Sandbox::touch_file(&dir.join("stdout"), 0o200)?;
        }
        if execution.stderr.is_some() {
            Sandbox::touch_file(&dir.join("stderr"), 0o200)?;
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
            Sandbox::touch_file(&dir.join("box").join(&path), 0o200)?;
        }
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
