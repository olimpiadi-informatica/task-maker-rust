use crate::languages::*;
use std::path::{Path, PathBuf};
use task_maker_dag::ExecutionLimits;

/// The Shell language
#[derive(Debug)]
pub struct LanguageShell;

impl LanguageShell {
    /// Make a new LanguageShell using the specified version.
    pub fn new() -> LanguageShell {
        LanguageShell {}
    }
}

impl Language for LanguageShell {
    fn name(&self) -> &'static str {
        "Shell"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["sh"]
    }

    fn need_compilation(&self) -> bool {
        false
    }

    fn custom_limits(&self, limits: &mut ExecutionLimits) {
        limits.nproc = None;
    }

    fn executable_name(&self, path: &Path, _write_to: Option<&Path>) -> PathBuf {
        // force the original extension for the file name
        PathBuf::from(path.file_name().expect("Invalid file name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;

    #[test]
    fn test_allow_fork() {
        let lang = LanguageShell::new();
        let mut limits = ExecutionLimits::unrestricted();
        limits.nproc(1);
        lang.custom_limits(&mut limits);
        assert_that!(limits.nproc).is_none();
    }
}
