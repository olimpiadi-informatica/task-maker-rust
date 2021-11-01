use std::path::Path;

use task_maker_dag::ExecutionCommand;

use crate::language::{
    CompilationSettings, CompiledLanguageBuilder, SimpleCompiledLanguageBuilder,
};
use crate::languages::cpp::find_cpp_deps;
use crate::Language;

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

    fn compilation_builder(
        &self,
        source: &Path,
        settings: CompilationSettings,
    ) -> Option<Box<dyn CompiledLanguageBuilder + '_>> {
        let mut metadata = SimpleCompiledLanguageBuilder::new(
            self,
            source,
            settings.clone(),
            self.config.compiler.clone(),
        );
        let binary_name = metadata.binary_name.clone();
        metadata
            .add_arg("-O2")
            .add_arg("-Wall")
            .add_arg("-ggdb3")
            .add_arg("-DEVAL")
            .add_arg("-fdiagnostics-color=always")
            .add_arg(format!("-std={}", self.config.std_version))
            .add_arg("-o")
            .add_arg(binary_name)
            .add_arg("-lm");
        for arg in &self.config.extra_flags {
            metadata.add_arg(arg);
        }
        if metadata.settings.list_static {
            metadata.add_arg("-static");
        }

        find_cpp_deps(source)
            .into_iter()
            .for_each(|d| metadata.add_dependency(d));
        Some(Box::new(metadata))
    }
}

#[cfg(test)]
mod tests {
    use spectral::prelude::*;
    use tempdir::TempDir;

    use task_maker_dag::ExecutionDAG;

    use super::*;

    fn setup() -> TempDir {
        let tempdir = TempDir::new("tm-test").unwrap();
        let foo = tempdir.path().join("foo.c");
        std::fs::write(foo, "int main() {}").unwrap();
        tempdir
    }

    #[test]
    fn test_compilation_args() {
        let tmp = setup();

        let lang = LanguageC::new(LanguageCConfiguration {
            compiler: ExecutionCommand::System("gcc".into()),
            std_version: "c11".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let mut builder = lang
            .compilation_builder(&tmp.path().join("foo.c"), CompilationSettings::default())
            .unwrap();
        let (comp, _exec) = builder.finalize(&mut ExecutionDAG::new()).unwrap();

        let args = comp.args;
        assert_that!(args).contains("foo.c".to_string());
        assert_that!(args).contains("-std=c11".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).does_not_contain("-static".to_string());
    }

    #[test]
    fn test_compilation_args_static() {
        let tmp = setup();

        let lang = LanguageC::new(LanguageCConfiguration {
            compiler: ExecutionCommand::System("gcc".into()),
            std_version: "c11".to_string(),
            extra_flags: vec!["-lfoobar".into()],
        });
        let mut settings = CompilationSettings::default();
        settings.list_static = true;
        let mut builder = lang
            .compilation_builder(&tmp.path().join("foo.c"), settings)
            .unwrap();
        let (comp, _exec) = builder.finalize(&mut ExecutionDAG::new()).unwrap();

        let args = comp.args;
        assert_that!(args).contains("foo.c".to_string());
        assert_that!(args).contains("-std=c11".to_string());
        assert_that!(args).contains("-lfoobar".to_string());
        assert_that!(args).contains("-static".to_string());
    }
}
