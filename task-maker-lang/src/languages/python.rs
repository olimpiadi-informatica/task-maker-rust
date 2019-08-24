use crate::languages::*;
use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Version of the Python interpreter to use.
#[allow(dead_code)]
#[derive(Debug)]
pub enum LanguagePythonVersion {
    /// Use the shebang written as the first line of the source.
    Autodetect,
    /// Force `python2`
    Python2,
    /// Force `python3`
    Python3,
}

/// The Python language
#[derive(Debug)]
pub struct LanguagePython {
    version: LanguagePythonVersion,
}

impl LanguagePython {
    /// Make a new LanguagePython using the specified version.
    pub fn new(version: LanguagePythonVersion) -> LanguagePython {
        LanguagePython { version }
    }
}

impl Language for LanguagePython {
    fn name(&self) -> &'static str {
        match self.version {
            LanguagePythonVersion::Autodetect => "Python / Autodetect",
            LanguagePythonVersion::Python2 => "Python2",
            LanguagePythonVersion::Python3 => "Python3",
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        return vec!["py"];
    }

    fn need_compilation(&self) -> bool {
        false
    }

    fn runtime_command(&self, path: &Path) -> ExecutionCommand {
        match self.version {
            LanguagePythonVersion::Autodetect => {
                ExecutionCommand::Local(self.executable_name(path))
            }
            LanguagePythonVersion::Python2 => ExecutionCommand::System(PathBuf::from("python2")),
            LanguagePythonVersion::Python3 => ExecutionCommand::System(PathBuf::from("python3")),
        }
    }

    fn runtime_args(&self, path: &Path, mut args: Vec<String>) -> Vec<String> {
        match self.version {
            LanguagePythonVersion::Autodetect => args,
            _ => {
                // will run for example: python3 program.py args...
                args.insert(0, self.executable_name(path).to_str().unwrap().to_owned());
                args
            }
        }
    }

    fn runtime_dependencies(&self, path: &Path) -> Vec<Dependency> {
        find_python_deps(path)
    }
}

/// Perform a BFS visit on the file dependencies looking for all the .py files
/// to add to the sandbox in order to execute the script. Will take only the
/// files that actually exist in the file's folder, ignoring the unresolved
/// ones.
fn find_python_deps(path: &Path) -> Vec<Dependency> {
    let base = path.parent().unwrap();
    let filename = path.file_name().unwrap();
    let mut result = vec![];
    let mut result_files = HashSet::new();
    let mut pending = VecDeque::new();
    let mut done = HashSet::new();

    pending.push_back(path.to_owned());
    while !pending.is_empty() {
        let path = pending.pop_front().unwrap();
        let imports = extract_imports(&path);
        done.insert(path);
        for import in imports {
            let import = PathBuf::from(format!("{}.py", import));
            let path = base.join(&import);
            if path.exists() && !done.contains(&path) && !result_files.contains(&import) {
                result_files.insert(import.clone());
                result.push(Dependency {
                    file: File::new(&format!("Dependency {:?} of {:?}", import, filename)),
                    local_path: path,
                    sandbox_path: import,
                    executable: false,
                });
            }
        }
    }

    result
}

/// Extracts all the imports in the file. The supported imports are the ones in
/// the form:
/// * import __file__
/// * from __file__ import stuff
/// * import __file1__, __file2__
fn extract_imports(path: &Path) -> Vec<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new("import +(.+)|from +(.+) +import").unwrap();
    }
    let content = if let Ok(content) = std::fs::read_to_string(path) {
        content
    } else {
        return vec![];
    };
    let mut res: Vec<String> = Vec::new();
    for cap in RE.captures_iter(&content) {
        if let Some(type1) = cap.get(1) {
            for piece in type1.as_str().split(',') {
                res.push(piece.trim().to_string());
            }
        } else if let Some(type2) = cap.get(2) {
            res.push(type2.as_str().to_owned());
        }
    }
    res
}
