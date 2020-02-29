use crate::languages::{Dependency, Language};
use crate::LanguageManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use task_maker_dag::*;

/// The storage of the compilation/runtime dependencies for the source files.
///
/// A source file may need some extra dependencies in order to be compiled and/or executed. For
/// example a C++ file may need a second C++ file to be linked together, or a Python file may need
/// a second Python file to be run.
#[derive(Debug, Serialize, Deserialize)]
pub struct GraderMap {
    /// The map from the name of the language to the file handle of the grader.
    graders: HashMap<String, Dependency>,
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
                    lang.name().into(),
                    Dependency {
                        file,
                        local_path: grader.clone(),
                        sandbox_path: PathBuf::from(grader.file_name().expect("Invalid file name")),
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
            vec![]
        } else {
            vec![self.graders[lang.name()].clone()]
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
            vec![]
        } else {
            vec![self.graders[lang.name()].clone()]
        }
    }

    /// Return an iterator over the paths of all the graders in this map.
    ///
    /// ```
    /// use task_maker_lang::{GraderMap, LanguageManager};
    /// use std::path::Path;
    ///
    /// let map = GraderMap::new(vec!["file.cpp"]);
    /// assert_eq!(map.all_paths().collect::<Vec<_>>(), vec![Path::new("file.cpp")]);
    /// ```
    pub fn all_paths(&self) -> impl Iterator<Item = &Path> {
        self.graders.values().map(|dep| dep.local_path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::c::{LanguageC, LanguageCConfiguration};
    use crate::languages::cpp::{LanguageCpp, LanguageCppConfiguration};
    use crate::languages::python::{LanguagePython, LanguagePythonVersion};
    use spectral::prelude::*;

    #[test]
    fn test_new() {
        let grader_map = GraderMap::new(vec!["grader.c", "grader.cpp", "grader.foobar"]);
        assert_that!(grader_map.graders).has_length(2);
    }

    #[test]
    fn test_get_compilation_deps() {
        let grader_map = GraderMap::new(vec!["grader.cpp", "grader.py"]);

        let lang = LanguageCpp::new(LanguageCppConfiguration::from_env());
        let deps = grader_map.get_compilation_deps(&lang);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("grader.cpp"));

        let lang = LanguageC::new(LanguageCConfiguration::from_env());
        let deps = grader_map.get_compilation_deps(&lang);
        assert_that!(deps).is_empty();

        let lang = LanguagePython::new(LanguagePythonVersion::Autodetect);
        let deps = grader_map.get_compilation_deps(&lang);
        assert_that!(deps).is_empty();
    }

    #[test]
    fn test_get_runtime_deps() {
        let grader_map = GraderMap::new(vec!["grader.cpp", "grader.py"]);

        let lang = LanguageCpp::new(LanguageCppConfiguration::from_env());
        let deps = grader_map.get_runtime_deps(&lang);
        assert_that!(deps).is_empty();

        let lang = LanguageC::new(LanguageCConfiguration::from_env());
        let deps = grader_map.get_runtime_deps(&lang);
        assert_that!(deps).is_empty();

        let lang = LanguagePython::new(LanguagePythonVersion::Autodetect);
        let deps = grader_map.get_runtime_deps(&lang);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("grader.py"));
    }

    #[test]
    fn test_all_paths() {
        let grader_map = GraderMap::new(vec!["grader.cpp", "grader.py"]);
        let paths: Vec<_> = grader_map.all_paths().collect();
        assert_that!(paths).has_length(2);
        assert_that!(paths).contains(Path::new("grader.cpp"));
        assert_that!(paths).contains(Path::new("grader.py"));
    }
}
