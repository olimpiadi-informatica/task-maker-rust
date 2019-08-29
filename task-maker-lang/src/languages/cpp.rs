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
        return vec!["cpp", "cc", "c++"];
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
        let mut args = vec![
            "-O2",
            "-Wall",
            "-ggdb3",
            "-DEVAL",
            "-o",
            exe_name.to_str().unwrap(),
        ];
        match self.version {
            LanguageCppVersion::GccCpp11 | LanguageCppVersion::ClangCpp11 => {
                args.push("-std=c++11")
            }
            LanguageCppVersion::GccCpp14 => args.push("-std=c++14"),
        }
        args.push(path.file_name().unwrap().to_str().unwrap());
        args.into_iter().map(|s| s.to_owned()).collect()
    }

    fn compilation_add_file(&self, mut args: Vec<String>, file: &Path) -> Vec<String> {
        args.push(file.to_str().unwrap().to_owned());
        args
    }

    /// The executable name is the source file's one without the extension.
    fn executable_name(&self, path: &Path) -> PathBuf {
        let name = PathBuf::from(path.file_name().unwrap());
        PathBuf::from(name.file_stem().unwrap())
    }
}
