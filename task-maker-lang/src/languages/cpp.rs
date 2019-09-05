use crate::languages::Language;
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

    fn compilation_command(&self, _path: &Path) -> ExecutionCommand {
        match self.version {
            LanguageCppVersion::GccCpp11 | LanguageCppVersion::GccCpp14 => {
                ExecutionCommand::system("g++")
            }
            LanguageCppVersion::ClangCpp11 => ExecutionCommand::system("clang++"),
        }
    }

    fn compilation_args(&self, path: &Path) -> Vec<String> {
        let exe_name = self.executable_name(path);
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

    /// The executable name is the source file's one without the extension.
    fn executable_name(&self, path: &Path) -> PathBuf {
        let name = PathBuf::from(path.file_name().expect("Invalid source file name"));
        PathBuf::from(name.file_stem().expect("Invalid source file name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;

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
}
