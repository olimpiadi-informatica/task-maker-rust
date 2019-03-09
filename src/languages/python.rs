use crate::execution::*;
use crate::languages::Language;
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
}
