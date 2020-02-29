//! Crate for managing programming languages and source files.
//!
//! This crate will expose some utility functions for making a DAG with the execution of commands
//! that come directly from source files.
//!
//! The [`Language`](languages/trait.Language.html) trait exposes the interface for defining new
//! programming languages. The list of supported programming languages can be found in the source of
//! this crate.
//!
//! The entry point of this crate is [`LanguageManager`](struct.LanguageManager.html), a struct that
//! is able to detect the language of a source file based on its extension. A trait object is used
//! to keep track of the language.
//!
//! To actually use the language you can use [`SourceFile`](struct.SourceFile.html), it exposes the
//! functionalities for compiling and running a source file.
//!
//! # Example
//!
//! ```
//! use task_maker_lang::LanguageManager;
//!
//! let lang = LanguageManager::detect_language("test.cpp").expect("unknown lang");
//! assert!(lang.name().contains("C++"))
//! ```

#![deny(missing_docs)]

#[macro_use]
extern crate lazy_static;

mod grader_map;
mod languages;
mod source_file;

pub use grader_map::GraderMap;
pub use languages::{Dependency, Language};
pub use source_file::SourceFile;

use languages::*;
use std::path::Path;
use std::sync::Arc;

/// Manager of all the known languages, you should use this to get
/// [`Language`](languages/trait.Language.html) instances.
pub struct LanguageManager {
    /// The list of all the known languages.
    known_languages: Vec<Arc<dyn Language + Sync + Send>>,
}

impl LanguageManager {
    /// Make a new `LanguageManager` with all the known languages.
    fn new() -> LanguageManager {
        LanguageManager {
            // ordered by most important first
            known_languages: vec![
                Arc::new(cpp::LanguageCpp::new(
                    cpp::LanguageCppConfiguration::from_env(),
                )),
                Arc::new(c::LanguageC::new(c::LanguageCConfiguration::from_env())),
                Arc::new(python::LanguagePython::new(
                    python::LanguagePythonVersion::Autodetect,
                )),
                Arc::new(shell::LanguageShell::new()),
                Arc::new(pascal::LanguagePascal::new()),
            ],
        }
    }

    /// Given a path to a file guess the language that the source file probably is.
    ///
    /// ```
    /// use task_maker_lang::LanguageManager;
    ///
    /// let cpp = LanguageManager::detect_language("test.cpp").unwrap();
    /// assert!(cpp.name().contains("C++")); // it's something like "C++11 / gcc"
    /// let py = LanguageManager::detect_language("test.py").unwrap();
    /// assert!(py.name().contains("Python")); // it's something like "Python / Autodetect"
    /// let unknown = LanguageManager::detect_language("test.foobar");
    /// assert!(unknown.is_none());
    /// ```
    pub fn detect_language<P: AsRef<Path>>(path: P) -> Option<Arc<dyn Language>> {
        let manager = &LANGUAGE_MANAGER_SINGL;
        let ext = path
            .as_ref()
            .extension()
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "".into())
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

    /// Search between the known languages the one with the specified name and return it if found.
    pub(crate) fn from_name<S: AsRef<str>>(name: S) -> Option<Arc<dyn Language>> {
        let manager = &LANGUAGE_MANAGER_SINGL;
        for lang in manager.known_languages.iter() {
            if lang.name() == name.as_ref() {
                return Some(lang.clone());
            }
        }
        None
    }
}

lazy_static! {
    /// The singleton instance of the `LanguageManager`.
    static ref LANGUAGE_MANAGER_SINGL: LanguageManager = LanguageManager::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::cpp::{LanguageCpp, LanguageCppConfiguration};
    use spectral::prelude::*;

    #[test]
    fn test_detect_language() {
        let lang = LanguageManager::detect_language("foo.cpp").unwrap();
        let name = LanguageCpp::new(LanguageCppConfiguration::from_env()).name();
        assert_that!(lang.name()).is_equal_to(name);
    }

    #[test]
    fn test_detect_language_uppercase() {
        let lang = LanguageManager::detect_language("foo.CPP").unwrap();
        let name = LanguageCpp::new(LanguageCppConfiguration::from_env()).name();
        assert_that!(lang.name()).is_equal_to(name);
    }

    #[test]
    fn test_detect_language_unknown() {
        let lang = LanguageManager::detect_language("foo.blah");
        assert_that!(lang).is_none();
    }

    #[test]
    fn test_from_name() {
        let name = LanguageCpp::new(LanguageCppConfiguration::from_env()).name();
        let lang = LanguageManager::from_name(name).unwrap();
        assert_that!(lang.name()).is_equal_to(name);
    }

    #[test]
    fn test_from_name_unknown() {
        let lang = LanguageManager::from_name("Nope, this is not a language");
        assert_that!(lang).is_none();
    }
}
