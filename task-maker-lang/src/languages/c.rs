use std::path::Path;

use task_maker_dag::*;

use crate::languages::cpp::find_cpp_deps;
use crate::languages::Language;
use crate::Dependency;

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
        vec!["c"]
    }

    fn need_compilation(&self) -> bool {
        true
    }

    fn compilation_command(&self, _path: &Path, _write_to: Option<&Path>) -> ExecutionCommand {
        ExecutionCommand::system("gcc")
    }

    fn compilation_args(&self, path: &Path, write_to: Option<&Path>) -> Vec<String> {
        let exe_name = self.compiled_file_name(path, write_to);
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
        args.push("-lm".to_string());
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

#[cfg(test)]
mod tests {
    use spectral::prelude::*;

    use super::*;

    #[test]
    fn test_compilation_args() {
        let lang = LanguageC::new(LanguageCVersion::GccC11);
        let args = lang.compilation_args(Path::new("foo.c"));
        assert_that!(args).contains("foo.c".to_string());
        assert_that!(args).contains("-std=c11".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("foo".to_string());
    }

    #[test]
    fn test_compilation_add_file() {
        let lang = LanguageC::new(LanguageCVersion::GccC11);
        let args = lang.compilation_args(Path::new("foo.c"));
        let new_args = lang.compilation_add_file(args.clone(), Path::new("bar.c"));
        assert_that!(new_args.iter()).contains_all_of(&args.iter());
        assert_that!(new_args.iter()).contains("bar.c".to_string());
    }

    #[test]
    fn test_executable_name() {
        let lang = LanguageC::new(LanguageCVersion::GccC11);
        assert_that!(lang.executable_name(Path::new("foo.c"))).is_equal_to(PathBuf::from("foo"));
    }
}
