use std::path::Path;

use task_maker_dag::*;

use crate::languages::cpp::find_cpp_deps;
use crate::languages::Language;
use crate::Dependency;

/// Configuration of the C language to use.
#[derive(Clone, Debug)]
pub struct LanguageCConfiguration {
    /// Compiler to use (e.g. ExecutionCommand::system("gcc") ).
    pub compiler: ExecutionCommand,
    /// Version of the C standard library to use (e.g. c11).
    pub std_version: String,
    /// Extra flags to pass to the compiler.
    pub extra_flags: Vec<String>,
}

/// The C language.
#[derive(Debug)]
pub struct LanguageC {
    pub config: LanguageCConfiguration,
}

impl LanguageC {
    /// Make a new LanguageC using the specified version.
    pub fn new(config: LanguageCConfiguration) -> LanguageC {
        LanguageC { config }
    }
}

impl LanguageCConfiguration {
    /// Get the configuration of C from the environment variables.
    pub fn from_env() -> LanguageCConfiguration {
        let compiler = std::env::var_os("TM_CC").unwrap_or_else(|| "gcc".into());
        let std_version = std::env::var("TM_CC_STD_VERSION").unwrap_or_else(|_| "c11".into());
        let extra_flags = std::env::var("TM_CFLAGS").unwrap_or_else(|_| String::new());
        let extra_flags = shell_words::split(&extra_flags).expect("Invalid $TM_CFLAGS");
        LanguageCConfiguration {
            compiler: ExecutionCommand::System(compiler.into()),
            std_version,
            extra_flags,
        }
    }
}

impl Language for LanguageC {
    fn name(&self) -> &'static str {
        "C"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["c"]
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
        let mut args = vec!["-O2", "-Wall", "-ggdb3", "-DEVAL", "-o", exe_name.as_ref()];
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
    use std::path::PathBuf;

    use spectral::prelude::*;

    use super::*;

    #[test]
    fn test_compilation_args() {
        let lang = LanguageC::new(LanguageCConfiguration {
            compiler: ExecutionCommand::System("gcc".into()),
            std_version: "c11".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let args = lang.compilation_args(Path::new("foo.c"), None, false);
        assert_that!(args).contains("foo.c".to_string());
        assert_that!(args).contains("-std=c11".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("compiled".to_string());
        assert_that!(args).does_not_contain("-static".to_string());
    }

    #[test]
    fn test_compilation_args_static() {
        let lang = LanguageC::new(LanguageCConfiguration {
            compiler: ExecutionCommand::System("gcc".into()),
            std_version: "c11".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let args = lang.compilation_args(Path::new("foo.c"), None, true);
        assert_that!(args).contains("foo.c".to_string());
        assert_that!(args).contains("-std=c11".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).contains("-o".to_string());
        assert_that!(args).contains("compiled".to_string());
        assert_that!(args).contains("-static".to_string());
    }

    #[test]
    fn test_compilation_add_file() {
        let lang = LanguageC::new(LanguageCConfiguration::from_env());
        let args = lang.compilation_args(Path::new("foo.c"), None, false);
        let new_args = lang.compilation_add_file(args.clone(), Path::new("bar.c"));
        assert_that!(new_args.iter()).contains_all_of(&args.iter());
        assert_that!(new_args.iter()).contains("bar.c".to_string());
    }

    #[test]
    fn test_executable_name() {
        let lang = LanguageC::new(LanguageCConfiguration::from_env());
        assert_that!(lang.executable_name(Path::new("foo.c"), None))
            .is_equal_to(PathBuf::from("foo"));
    }
}
