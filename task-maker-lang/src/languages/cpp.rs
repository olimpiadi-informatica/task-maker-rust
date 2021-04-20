use std::path::{Path, PathBuf};

use regex::Regex;

use task_maker_dag::*;

use crate::languages::{find_dependencies, Language};
use crate::Dependency;

/// Configuration of the C++ language to use.
#[derive(Clone, Debug)]
pub struct LanguageCppConfiguration {
    /// Compiler to use (e.g. ExecutionCommand::system("g++") ).
    pub compiler: ExecutionCommand,
    /// Version of the C++ standard library to use (e.g. c++11).
    pub std_version: String,
    /// Extra flags to pass to the compiler.
    pub extra_flags: Vec<String>,
}

/// The C++ language.
#[derive(Debug)]
pub struct LanguageCpp {
    pub config: LanguageCppConfiguration,
}

impl LanguageCppConfiguration {
    /// Get the configuration of C++ from the environment variables.
    pub fn from_env() -> LanguageCppConfiguration {
        let compiler = std::env::var_os("TM_CXX").unwrap_or_else(|| "g++".into());
        let std_version = std::env::var("TM_CXX_STD_VERSION").unwrap_or_else(|_| "c++11".into());
        let extra_flags = std::env::var("TM_CXXFLAGS").unwrap_or_else(|_| String::new());
        let extra_flags = shell_words::split(&extra_flags).expect("Invalid $TM_CXXFLAGS");
        LanguageCppConfiguration {
            compiler: ExecutionCommand::System(compiler.into()),
            std_version,
            extra_flags,
        }
    }
}

impl LanguageCpp {
    /// Make a new LanguageCpp using the specified version.
    pub fn new(config: LanguageCppConfiguration) -> LanguageCpp {
        LanguageCpp { config }
    }
}

impl Language for LanguageCpp {
    fn name(&self) -> &'static str {
        "C++"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["cpp", "cc", "c++"]
    }

    fn need_compilation(&self) -> bool {
        true
    }

    fn compilation_command(&self, _path: &Path, _write_to: Option<&Path>) -> ExecutionCommand {
        self.config.compiler.clone()
    }

    fn compilation_args(
        &self,
        path: &Path,
        write_to: Option<&Path>,
        link_static: bool,
    ) -> Vec<String> {
        let exe_name = self.compiled_file_name(path, write_to);
        let exe_name = exe_name.to_string_lossy();
        let mut args = vec![
            "-O2",
            "-Wall",
            "-ggdb3",
            "-DEVAL",
            "-fdiagnostics-color=always",
            "-o",
            exe_name.as_ref(),
        ];
        if link_static {
            args.push("-static");
        }
        let mut args: Vec<_> = args.into_iter().map(|s| s.to_string()).collect();
        args.push(format!("-std={}", self.config.std_version));
        for arg in &self.config.extra_flags {
            args.push(arg.clone());
        }
        args.push(
            path.file_name()
                .expect("Invalid source file name")
                .to_string_lossy()
                .to_string(),
        );
        args
    }

    fn compilation_add_file(&self, mut args: Vec<String>, file: &Path) -> Vec<String> {
        args.push(file.to_string_lossy().to_string());
        args
    }

    fn compilation_dependencies(&self, path: &Path) -> Vec<Dependency> {
        find_cpp_deps(path)
    }
}

/// Extract all the dependencies of a C/C++ source file.
pub(crate) fn find_cpp_deps(path: &Path) -> Vec<Dependency> {
    find_dependencies(path, extract_includes)
}

/// Extracts all the #include in the file. The supported includes are the ones in the form:
/// * `#include <file>`
/// * `#include "file"`
///
/// The space after `include` is optional.
///
/// The returned values are in the form (local_path, sandbox_path). Those paths are equal, and are
/// just the include itself.
fn extract_includes(path: &Path) -> Vec<(PathBuf, PathBuf)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"#include\s*[<"]([^">]+)[>"]"#).expect("Invalid regex");
    }
    let path = match path.canonicalize() {
        Ok(path) => path,
        _ => return vec![],
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        _ => return vec![],
    };
    let mut res = Vec::new();
    let mut add_file = |include: PathBuf| {
        let local = path.with_file_name(&include);
        res.push((local, include));
    };
    for cap in RE.captures_iter(&content) {
        if let Some(path) = cap.get(1) {
            add_file(PathBuf::from(path.as_str()));
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use spectral::prelude::*;

    use super::*;

    #[test]
    fn test_compilation_args() {
        let lang = LanguageCpp::new(LanguageCppConfiguration {
            compiler: ExecutionCommand::System("g++".into()),
            std_version: "c++14".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let args = lang.compilation_args(Path::new("foo.cpp"), None, false);
        assert_that!(args).contains("foo.cpp".to_string());
        assert_that!(args).contains("-std=c++14".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("compiled".to_string());
        assert_that!(args).does_not_contain("-static".to_string());
    }

    #[test]
    fn test_compilation_args_static() {
        let lang = LanguageCpp::new(LanguageCppConfiguration {
            compiler: ExecutionCommand::System("g++".into()),
            std_version: "c++14".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let args = lang.compilation_args(Path::new("foo.cpp"), None, true);
        assert_that!(args).contains("foo.cpp".to_string());
        assert_that!(args).contains("-std=c++14".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("compiled".to_string());
        assert_that!(args).contains("-static".to_string());
    }

    #[test]
    fn test_compilation_add_file() {
        let lang = LanguageCpp::new(LanguageCppConfiguration::from_env());
        let args = lang.compilation_args(Path::new("foo.cpp"), None, false);
        let new_args = lang.compilation_add_file(args.clone(), Path::new("bar.cpp"));
        assert_that!(new_args.iter()).contains_all_of(&args.iter());
        assert_that!(new_args.iter()).contains("bar.cpp".to_string());
    }

    #[test]
    fn test_executable_name() {
        let lang = LanguageCpp::new(LanguageCppConfiguration::from_env());
        assert_that!(lang.executable_name(Path::new("foo.cpp"), None))
            .is_equal_to(PathBuf::from("foo"));
    }

    #[test]
    fn test_extract_imports() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("file.cpp");
        write(
            &path,
            r#"blabla\n#include<iostream>\n#include "test.hpp"\nrandom"#,
        )
        .unwrap();
        let imports = extract_includes(&path);
        for (i, import) in vec!["iostream", "test.hpp"].iter().enumerate() {
            let import = PathBuf::from(import);
            assert_that!(imports[i].0).is_equal_to(tmpdir.path().join(&import));
            assert_that!(imports[i].1).is_equal_to(&import);
        }
    }

    #[test]
    fn test_find_cpp_deps() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("file.cpp");
        let foo_path = tmpdir.path().join("foo.hpp");
        let bar_path = tmpdir.path().join("bar.hpp");
        write(&path, "#include <foo.hpp>").unwrap();
        write(&foo_path, "#include <bar.hpp>").unwrap();
        write(&bar_path, "#include <iostream>").unwrap();
        let deps = find_cpp_deps(&path);
        assert_that!(deps).has_length(2);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.hpp"));
        assert_that!(deps[1].local_path).is_equal_to(bar_path);
        assert_that!(deps[1].sandbox_path).is_equal_to(PathBuf::from("bar.hpp"));
    }

    #[test]
    fn test_find_cpp_deps_loop() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let file_path = tmpdir.path().join("file.cpp");
        let foo_path = tmpdir.path().join("foo.hpp");
        // file imports itself and foo and file import each other
        write(&file_path, "#include <file.cpp>\n#include<foo.hpp>").unwrap();
        write(&foo_path, "#include\"file.cpp\"").unwrap();
        let deps = find_cpp_deps(&file_path);
        assert_that!(deps).has_length(1);
        assert_that!(deps[0].local_path).is_equal_to(foo_path);
        assert_that!(deps[0].sandbox_path).is_equal_to(PathBuf::from("foo.hpp"));
    }
}
