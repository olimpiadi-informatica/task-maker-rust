use crate::languages::*;
use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use task_maker_dag::{ExecutionCommand, File};

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
        vec!["py"]
    }

    fn need_compilation(&self) -> bool {
        false
    }

    fn runtime_command(&self, path: &Path) -> ExecutionCommand {
        match self.version {
            LanguagePythonVersion::Autodetect => {
                ExecutionCommand::local(self.executable_name(path))
            }
            LanguagePythonVersion::Python2 => ExecutionCommand::system("python2"),
            LanguagePythonVersion::Python3 => ExecutionCommand::system("python3"),
        }
    }

    fn runtime_args(&self, path: &Path, mut args: Vec<String>) -> Vec<String> {
        match self.version {
            LanguagePythonVersion::Autodetect => args,
            _ => {
                // will run for example: python3 program.py args...
                args.insert(0, self.executable_name(path).to_string_lossy().to_string());
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
    let base = path.parent().expect("Invalid path");
    let filename = path.file_name().expect("Invalid path");
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
        static ref RE: Regex =
            Regex::new("import +(.+)|from +(.+) +import").expect("Invalid regex");
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

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;
    use std::fs::write;

    #[test]
    fn test_runtime_args_autodetect() {
        let lang = LanguagePython::new(LanguagePythonVersion::Autodetect);
        let path = Path::new("script.py");
        assert_that!(lang.runtime_command(path)).is_equal_to(ExecutionCommand::local(path));
        let args = lang.runtime_args(path, vec!["arg".to_string()]);
        assert_that!(&args).is_equal_to(vec!["arg".to_string()]);
    }

    #[test]
    fn test_runtime_args_py3() {
        let lang = LanguagePython::new(LanguagePythonVersion::Python3);
        let path = Path::new("script.py");
        assert_that!(lang.runtime_command(path)).is_equal_to(ExecutionCommand::system("python3"));
        let args = lang.runtime_args(path, vec!["arg".to_string()]);
        assert_that!(&args).is_equal_to(vec!["script.py".to_string(), "arg".to_string()]);
    }

    #[test]
    fn test_extract_imports() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("script.py");
        write(
            &path,
            "random stuff\nimport foo\nfrom bar import xxx\nimport baz, biz",
        )
        .unwrap();
        let imports = extract_imports(&path);
        assert_that!(imports).is_equal_to(vec![
            "foo".to_string(),
            "bar".to_string(),
            "baz".to_string(),
            "biz".to_string(),
        ]);
    }

    #[test]
    fn test_find_python_deps() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("script.py");
        let foo_path = tmpdir.path().join("foo.py");
        write(&path, "import foo").unwrap();
        write(&foo_path, "import not_found").unwrap();
        let deps = find_python_deps(&path);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.py"));
    }

    #[test]
    fn test_find_python_deps_loop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("script.py");
        let foo_path = tmpdir.path().join("foo.py");
        write(&path, "import foo").unwrap();
        write(&foo_path, "import script").unwrap();
        let deps = find_python_deps(&path);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.py"));
    }
}
