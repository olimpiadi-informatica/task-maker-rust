use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use task_maker_dag::*;

pub(crate) mod c;
pub(crate) mod cpp;
pub(crate) mod python;
pub(crate) mod shell;

/// A dependency of an execution, all the sandbox paths must be relative and inside of the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// The handle of the file.
    pub file: File,
    /// The path of the file on the local system.
    pub local_path: PathBuf,
    /// The path inside of the sandbox of where to put the file. Must be relative to the sandbox and
    /// inside of it.
    pub sandbox_path: PathBuf,
    /// Whether the file should be executable or not.
    pub executable: bool,
}

/// Trait that defines the properties of the supported languages. Most of the methods have a safe
/// blanket implementation, note that not all of them are _really_ optional: based on the value
/// returned by `need_compilation` some of the methods become required.
///
/// A language can be either compiler or not-compiler.
///
/// When a language is compiled the extra required implementations are:
/// - `compilation_command`
/// - `compilation_args`
/// - `compilation_add_file`
pub trait Language: std::fmt::Debug + Send + Sync {
    /// Full name of the language. This must be unique between all the other languages.
    fn name(&self) -> &'static str;

    /// List of valid extensions for this language. A file is considered in this language if its
    /// extension is inside this list.
    fn extensions(&self) -> Vec<&'static str>;

    /// Whether this language needs a compilation step. Returning `true` here triggers many changes
    /// in the behaviour of the execution. Of course the compilation step will be added, because of
    /// that there is the need to know how to compile the source file, forcing the implementation of
    /// some extra methods.
    fn need_compilation(&self) -> bool;

    /// Command to use to compile the source file. The blanked implementation is intended for not
    /// compiled languages.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_command(&self, _path: &Path) -> ExecutionCommand {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// Arguments to pass to the compiler to compile to source file. The source file is located at
    /// `path.file_name()` inside the sandbox and the result of the compilation should placed at
    /// `self.executable_name(path)`. The blanked implementation is intended for not compiled
    /// languages.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_args(&self, _path: &Path) -> Vec<String> {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// Add a file to the compilation command if the language requires that. That file can be any
    /// compile time dependency and it's relative to the sandbox.
    ///
    /// The new compilation arguments should be returned.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_add_file(&self, _args: Vec<String>, _file: &Path) -> Vec<String> {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// The dependencies to put inside the compilation sandbox. This does not include the source
    /// file.
    fn compilation_dependencies(&self, _path: &Path) -> Vec<Dependency> {
        vec![]
    }

    /// Command to use to run the program. It defaults to the executable name of the program.
    /// Languages that need to run a separate program (e.g. a system-wise interpreter) may change
    /// the return value of this method.
    fn runtime_command(&self, path: &Path) -> ExecutionCommand {
        ExecutionCommand::Local(self.executable_name(path))
    }

    /// Arguments to pass to the executable to start the evaluation.
    fn runtime_args(&self, _path: &Path, args: Vec<String>) -> Vec<String> {
        args
    }

    /// Add a file to the runtime command if the language requires that. That file can be any run
    /// time dependency and it's relative to the sandbox.
    ///
    /// The new runtime arguments should be returned.
    fn runtime_add_file(&self, args: Vec<String>, _file: &Path) -> Vec<String> {
        args
    }

    /// The dependencies to put inside the execution sandbox. This does not include the executable.
    fn runtime_dependencies(&self, _path: &Path) -> Vec<Dependency> {
        vec![]
    }

    /// The name of the executable to call inside the sandbox. It defaults to the file name of
    /// program.
    fn executable_name(&self, path: &Path) -> PathBuf {
        PathBuf::from(path.file_name().unwrap())
    }
}
