use crate::execution::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod cpp;
mod python;
mod shell;

/// A dependency of an execution, all the sandbox paths must be relative and
/// inside of the sandbox.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// The handle of the file.
    pub file: File,
    /// The path of the file on the local system.
    pub local_path: PathBuf,
    /// The path inside of the sandbox of where to put the file. Must be
    /// relative to the sandbox and inside of it.
    pub sandbox_path: PathBuf,
    /// Whether the file should be executable or not.
    pub executable: bool,
}

/// Trait that defines what a language is.
pub trait Language: std::fmt::Debug {
    /// Full name of the language
    fn name(&self) -> &'static str;

    /// List of valid extensions for this language
    fn extensions(&self) -> Vec<&'static str>;

    /// Whether this language needs a compilation step
    fn need_compilation(&self) -> bool;

    /// Command to use to compile the source file. The result of the
    /// compilation should be at `self.executable_name(path)`.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_command(&self, _path: &Path) -> ExecutionCommand {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// Arguments to pass to the compiler to compile to source file. The source
    /// file is located at `path.file_name()` inside the sandbox.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_args(&self, _path: &Path) -> Vec<String> {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// Add a file to the compilation command if the language requires that.
    /// That file can be any compile time dependency and it's relative to the
    /// sandbox.
    ///
    /// Will panic if this language does not support compilation.
    fn compilation_add_file(&self, _args: Vec<String>, _file: &Path) -> Vec<String> {
        panic!("Language {} cannot be compiled!", self.name());
    }

    /// The dependencies to put inside the compilation sandbox. This does not
    /// include the source file.
    fn compilation_dependencies(&self, _path: &Path) -> Vec<Dependency> {
        vec![]
    }

    /// Command to use to run the program.
    fn runtime_command(&self, path: &Path) -> ExecutionCommand {
        ExecutionCommand::Local(self.executable_name(path))
    }

    /// Arguments to pass to the executable to start the evaluation
    fn runtime_args(&self, _path: &Path, args: Vec<String>) -> Vec<String> {
        args
    }

    /// Add a file to the runtime command if the language requires that.
    /// That file can be any run time dependency and it's relative to the
    /// sandbox.
    fn runtime_add_file(&self, args: Vec<String>, _file: &Path) -> Vec<String> {
        args
    }

    /// The dependencies to put inside the execution sandbox. This does not
    /// include the executable.
    fn runtime_dependencies(&self, _path: &Path) -> Vec<Dependency> {
        vec![]
    }

    /// The name of the executable to call inside the sandbox
    fn executable_name(&self, path: &Path) -> PathBuf {
        PathBuf::from(path.file_name().unwrap())
    }
}

/// Singleton that manages the known languages
pub struct LanguageManager {
    /// The list of known languages
    known_languages: Vec<Arc<Language + Sync + Send>>,
}

impl LanguageManager {
    /// Make a new LanguageManager with all the known languages
    fn new() -> LanguageManager {
        LanguageManager {
            // ordered by most important first
            known_languages: vec![
                Arc::new(cpp::LanguageCpp::new(cpp::LanguageCppVersion::GccCpp14)),
                Arc::new(python::LanguagePython::new(
                    python::LanguagePythonVersion::Autodetect,
                )),
                Arc::new(shell::LanguageShell::new()),
            ],
        }
    }

    /// Given a path to an existing file detect the best language that the
    /// source file is.
    pub fn detect_language(path: &Path) -> Option<Arc<Language>> {
        let manager = &LANGUAGE_MANAGER_SINGL;
        let ext = path
            .extension()
            .unwrap_or_else(|| std::ffi::OsStr::new(""))
            .to_str()
            .unwrap()
            .to_lowercase();
        for lang in manager.known_languages.iter() {
            for lang_ext in lang.extensions().iter() {
                if ext == *lang_ext {
                    return Some(lang.clone());
                }
            }
        }
        None
    }
}

lazy_static! {
    /// The sigleton instance of the LanguageManager
    static ref LANGUAGE_MANAGER_SINGL: LanguageManager = LanguageManager::new();
}
