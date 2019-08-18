use crate::languages::{Dependency, Language, LanguageManager};
use std::collections::HashMap;
use std::path::PathBuf;
use task_maker_dag::*;

/// The storage of the compilation/runtime graders of the solutions.
#[derive(Debug)]
pub struct GraderMap {
    /// The map from the name of the language to the file handle of the grader.
    graders: HashMap<&'static str, Dependency>,
}

impl GraderMap {
    /// Make a new map with the speicified graders.
    pub fn new(graders: Vec<PathBuf>) -> GraderMap {
        let mut map = GraderMap {
            graders: HashMap::new(),
        };
        for grader in graders {
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

    /// Extra compilation dependencies of the graders, will be an empty Vec if
    /// the language is not compiled.
    pub fn get_compilation_deps(&self, lang: &Language) -> Vec<Dependency> {
        if !lang.need_compilation() || !self.graders.contains_key(lang.name()) {
            return vec![];
        } else {
            return vec![self.graders[lang.name()].clone()];
        }
    }

    /// Extra runtime dependencies of the graders, will be an empty Vec if the
    /// language is compiled.
    pub fn get_runtime_deps(&self, lang: &Language) -> Vec<Dependency> {
        if lang.need_compilation() || !self.graders.contains_key(lang.name()) {
            return vec![];
        } else {
            return vec![self.graders[lang.name()].clone()];
        }
    }
}
