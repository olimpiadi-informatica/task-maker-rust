use crate::execution::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod cpp;
mod python;

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

    /// Command to use to run the program.
    fn runtime_command(&self, path: &Path) -> ExecutionCommand {
        ExecutionCommand::Local(self.executable_name(path))
    }

    /// Arguments to pass to the executable to start the evaluation
    fn runtime_args(&self, _path: &Path, args: Vec<String>) -> Vec<String> {
        args
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
            ],
        }
    }

    /// Given a path to an existing file detect the best language that the
    /// source file is.
    pub fn detect_language(path: &Path) -> Option<Arc<Language>> {
        let manager = &LANGUAGE_MANAGER_SINGL;
        let ext = path
            .extension()
            .unwrap_or(std::ffi::OsStr::new(""))
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
