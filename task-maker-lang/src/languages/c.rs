use crate::languages::Language;
use std::path::{Path, PathBuf};
use task_maker_dag::*;

/// Version of the C standard and compiler to use.
#[allow(dead_code)]
#[derive(Debug)]
pub enum LanguageCVersion {
    /// gcc with -std=c99
    GccC99,
    /// gcc with -std=c11
    GccC11,
}

/// The C language.
#[derive(Debug)]
pub struct LanguageC {
    pub version: LanguageCVersion,
}

impl LanguageC {
    /// Make a new LanguageC using the specified version.
    pub fn new(version: LanguageCVersion) -> LanguageC {
        LanguageC { version }
    }
}

impl Language for LanguageC {
    fn name(&self) -> &'static str {
        match self.version {
            LanguageCVersion::GccC99 => "C99 / gcc",
            LanguageCVersion::GccC11 => "C11 / gcc",
        }
    }

    fn extensions(&self) -> Vec<&'static str> {
        return vec!["c"];
    }

    fn need_compilation(&self) -> bool {
        true
    }

    fn compilation_command(&self, _path: &Path) -> ExecutionCommand {
        ExecutionCommand::system("gcc")
    }

    fn compilation_args(&self, path: &Path) -> Vec<String> {
        let exe_name = self.executable_name(path);
        let exe_name = exe_name.to_string_lossy();
        let mut args = vec!["-O2", "-Wall", "-ggdb3", "-DEVAL", "-o", exe_name.as_ref()];
        match self.version {
            LanguageCVersion::GccC99 => args.push("-std=c99"),
            LanguageCVersion::GccC11 => args.push("-std=c11"),
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
