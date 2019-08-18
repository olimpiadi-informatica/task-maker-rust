use crate::languages::{Dependency, Language};
use crate::LanguageManager;
use std::collections::HashMap;
use std::path::PathBuf;
use task_maker_dag::*;

/// The storage of the compilation/runtime dependencies for the source files.
///
/// A source file may need some extra dependencies in order to be compiled and/or executed. For
/// example a C++ file may need a second C++ file to be linked together, or a Python file may need
/// a second Python file to be run.
#[derive(Debug)]
pub struct GraderMap {
    /// The map from the name of the language to the file handle of the grader.
    graders: HashMap<&'static str, Dependency>,
}

impl GraderMap {
    /// Make a new map with the specified graders.
    ///
    /// ```
    /// use task_maker_lang::GraderMap;
    ///
    /// let map = GraderMap::new(vec!["file.cpp", "file.py"]);
    /// ```
    pub fn new<P: Into<PathBuf>>(graders: Vec<P>) -> GraderMap {
        let mut map = GraderMap {
            graders: HashMap::new(),
        };
        for grader in graders {
            let grader = grader.into();
            let lang = LanguageManager::detect_language(&grader);
            if let Some(lang) = lang {
                let file = File::new(&format!("Grader for {}", lang.name()));
                map.graders.insert(
                    lang.name(),
                    Dependency {
                        file,
                        local_path: grader.clone(),
                        sandbox_path: PathBuf::from(grader.file_name().unwrap()),
                        executable: false,
                    },
                );
            }
        }
        map
    }

    /// Extra compilation dependencies of the graders, will be an empty `Vec` if the language is not
    /// compiled.
    ///
    /// ```
    /// use task_maker_lang::{GraderMap, LanguageManager};
    ///
    /// let map = GraderMap::new(vec!["file.cpp", "file.py"]);
    /// let cpp = LanguageManager::detect_language("source.cpp").unwrap();
    /// let py = LanguageManager::detect_language("source.py").unwrap();
    /// assert_eq!(map.get_compilation_deps(cpp.as_ref()).len(), 1);
    /// assert_eq!(map.get_compilation_deps(py.as_ref()).len(), 0);
    /// ```
    pub fn get_compilation_deps(&self, lang: &dyn Language) -> Vec<Dependency> {
        if !lang.need_compilation() || !self.graders.contains_key(lang.name()) {
            return vec![];
        } else {
            return vec![self.graders[lang.name()].clone()];
        }
    }

    /// Extra runtime dependencies of the graders, will be an empty `Vec` if the language is
    /// compiled.
    ///
    /// ```
    /// use task_maker_lang::{GraderMap, LanguageManager};
    ///
    /// let map = GraderMap::new(vec!["file.cpp", "file.py"]);
    /// let cpp = LanguageManager::detect_language("source.cpp").unwrap();
    /// let py = LanguageManager::detect_language("source.py").unwrap();
    /// assert_eq!(map.get_runtime_deps(cpp.as_ref()).len(), 0);
    /// assert_eq!(map.get_runtime_deps(py.as_ref()).len(), 1);
    /// ```
    pub fn get_runtime_deps(&self, lang: &dyn Language) -> Vec<Dependency> {
        if lang.need_compilation() || !self.graders.contains_key(lang.name()) {
            return vec![];
        } else {
            return vec![self.graders[lang.name()].clone()];
        }
    }
}
