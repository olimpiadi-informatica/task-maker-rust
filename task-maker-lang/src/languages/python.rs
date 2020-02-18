use crate::languages::*;
use regex::Regex;
use std::path::{Path, PathBuf};
use task_maker_dag::ExecutionCommand;

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

    fn runtime_command(&self, path: &Path, write_to: Option<&Path>) -> ExecutionCommand {
        match self.version {
            LanguagePythonVersion::Autodetect => {
                ExecutionCommand::local(self.executable_name(path, write_to))
            }
            LanguagePythonVersion::Python2 => ExecutionCommand::system("python2"),
            LanguagePythonVersion::Python3 => ExecutionCommand::system("python3"),
        }
    }

    fn runtime_args(
        &self,
        path: &Path,
        write_to: Option<&Path>,
        mut args: Vec<String>,
    ) -> Vec<String> {
        match self.version {
            LanguagePythonVersion::Autodetect => args,
            _ => {
                // will run for example: python3 program.py args...
                args.insert(
                    0,
                    self.executable_name(path, write_to)
                        .to_string_lossy()
                        .to_string(),
                );
                args
            }
        }
    }

    fn runtime_dependencies(&self, path: &Path) -> Vec<Dependency> {
        find_python_deps(path)
    }
}

/// Extract all the dependencies of a python file recursively.
fn find_python_deps(path: &Path) -> Vec<Dependency> {
    find_dependencies(path, extract_imports)
}

/// Extracts all the imports in the file. The supported imports are the ones in
/// the form:
/// * `import __file__`
/// * `from __file__ import stuff`
/// * `import __file1__, __file2__`
///
/// The returned values are in the form (local_path, sandbox_path). Those paths
/// are equal, and are just the import followed by `.py`, because of that the
/// imports should not contain a dot (i.e. python modules are not supported).
fn extract_imports(path: &Path) -> Vec<(PathBuf, PathBuf)> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new("import +(.+)|from +(.+) +import").expect("Invalid regex");
    }
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        _ => return vec![],
    };
    let mut res = Vec::new();
    for cap in RE.captures_iter(&content) {
        if let Some(type1) = cap.get(1) {
            for piece in type1.as_str().split(',') {
                let path = PathBuf::from(format!("{}.py", piece.trim()));
                res.push((path.clone(), path));
            }
        } else if let Some(type2) = cap.get(2) {
            let path = PathBuf::from(format!("{}.py", type2.as_str().trim()));
            res.push((path.clone(), path));
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
        let write_to = Path::new("script.boh");
        assert_that!(lang.runtime_command(path, Some(write_to)))
            .is_equal_to(ExecutionCommand::local(write_to));
        let args = lang.runtime_args(path, None, vec!["arg".to_string()]);
        assert_that!(&args).is_equal_to(vec!["arg".to_string()]);
    }

    #[test]
    fn test_runtime_args_py3() {
        let lang = LanguagePython::new(LanguagePythonVersion::Python3);
        let path = Path::new("script.py");
        let write_to = Path::new("script.boh");
        assert_that!(lang.runtime_command(path, Some(write_to)))
            .is_equal_to(ExecutionCommand::system("python3"));
        let args = lang.runtime_args(path, Some(write_to), vec!["arg".to_string()]);
        assert_that!(&args).is_equal_to(vec!["script.boh".to_string(), "arg".to_string()]);
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
        for (i, import) in vec!["foo", "bar", "baz", "biz"].iter().enumerate() {
            let import = PathBuf::from(format!("{}.py", import));
            assert_that!(imports[i]).is_equal_to((import.clone(), import));
        }
    }

    #[test]
    fn test_find_python_deps() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("script.py");
        let foo_path = tmpdir.path().join("foo.py");
        let bar_path = tmpdir.path().join("bar.py");
        write(&path, "import foo").unwrap();
        write(&foo_path, "import bar").unwrap();
        write(&bar_path, "import not_found").unwrap();
        let deps = find_python_deps(&path);
        assert_that!(deps).has_length(2);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.py"));
        assert_that!(deps[1].local_path).is_equal_to(bar_path);
        assert_that!(deps[1].sandbox_path).is_equal_to(PathBuf::from("bar.py"));
    }

    #[test]
    fn test_find_python_deps_loop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let script_path = tmpdir.path().join("script.py");
        let foo_path = tmpdir.path().join("foo.py");
        // script imports itself and foo and script import each other
        write(&script_path, "import foo\nimport script").unwrap();
        write(&foo_path, "import script").unwrap();
        let deps = find_python_deps(&script_path);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.py"));
    }
}
