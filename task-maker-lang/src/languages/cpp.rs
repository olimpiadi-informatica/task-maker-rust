use crate::languages::{find_dependencies, Language};
use crate::Dependency;
use regex::Regex;
use std::path::{Path, PathBuf};
use task_maker_dag::*;

/// Version of the C++ standard and compiler to use.
#[allow(dead_code)]
#[derive(Debug)]
pub enum LanguageCppVersion {
    /// g++ with -std=c++11
    GccCpp11,
    /// g++ with -std=c++14
    GccCpp14,
    /// clang++ with -std=c++11
    ClangCpp11,
}

/// The C++ language.
#[derive(Debug)]
pub struct LanguageCpp {
    pub version: LanguageCppVersion,
}

impl LanguageCpp {
    /// Make a new LanguageCpp using the specified version.
    pub fn new(version: LanguageCppVersion) -> LanguageCpp {
        LanguageCpp { version }
    }
}

impl Language for LanguageCpp {
    fn name(&self) -> &'static str {
        match self.version {
            LanguageCppVersion::GccCpp11 => "C++11 / gcc",
            LanguageCppVersion::GccCpp14 => "C++14 / gcc",
            LanguageCppVersion::ClangCpp11 => "C++11 / clang",
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["cpp", "cc", "c++"]
    }

    fn need_compilation(&self) -> bool {
        true
    }

    fn compilation_command(&self, _path: &Path, _write_to: Option<&Path>) -> ExecutionCommand {
        match self.version {
            LanguageCppVersion::GccCpp11 | LanguageCppVersion::GccCpp14 => {
                ExecutionCommand::system("g++")
            }
            LanguageCppVersion::ClangCpp11 => ExecutionCommand::system("clang++"),
        }
    }

    fn compilation_args(&self, path: &Path, write_to: Option<&Path>) -> Vec<String> {
        let exe_name = self.compiled_file_name(path, write_to);
        let exe_name = exe_name.to_string_lossy();
        let mut args = vec!["-O2", "-Wall", "-ggdb3", "-DEVAL", "-o", exe_name.as_ref()];
        match self.version {
            LanguageCppVersion::GccCpp11 | LanguageCppVersion::ClangCpp11 => {
                args.push("-std=c++11")
            }
            LanguageCppVersion::GccCpp14 => args.push("-std=c++14"),
        }
        let mut args: Vec<_> = args.into_iter().map(|s| s.to_string()).collect();
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
    use super::*;
    use spectral::prelude::*;
    use std::fs::write;

    #[test]
    fn test_compilation_args() {
        let lang = LanguageCpp::new(LanguageCppVersion::GccCpp14);
        let args = lang.compilation_args(Path::new("foo.cpp"));
        assert_that!(args).contains("foo.cpp".to_string());
        assert_that!(args).contains("-std=c++14".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("foo".to_string());
    }

    #[test]
    fn test_compilation_add_file() {
        let lang = LanguageCpp::new(LanguageCppVersion::GccCpp14);
        let args = lang.compilation_args(Path::new("foo.cpp"));
        let new_args = lang.compilation_add_file(args.clone(), Path::new("bar.cpp"));
        assert_that!(new_args.iter()).contains_all_of(&args.iter());
        assert_that!(new_args.iter()).contains("bar.cpp".to_string());
    }

    #[test]
    fn test_executable_name() {
        let lang = LanguageCpp::new(LanguageCppVersion::GccCpp14);
        assert_that!(lang.executable_name(Path::new("foo.cpp"))).is_equal_to(PathBuf::from("foo"));
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
