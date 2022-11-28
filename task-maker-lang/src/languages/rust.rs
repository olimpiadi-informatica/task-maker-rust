use std::path::{Path, PathBuf};
use task_maker_dag::ExecutionCommand;

use crate::language::{
    CompilationSettings, CompiledLanguageBuilder, Language, SimpleCompiledLanguageBuilder,
};

/// Configuration of the Rust language to use.
#[derive(Clone, Debug)]
pub struct LanguageRustConfiguration {
    /// Path to the rustup home in the worker.
    ///
    /// We cannot know if and where rustup is installed in the worker machines, and we cannot simply
    /// expose `$HOME` and `$RUSTUP_HOME` because this has to make the entire `/home` readable.
    /// Therefore, if you want to use rustup in the worker you have to clearly state here its path.
    ///
    /// If `$RUSTUP_HOME` is set, this should be its value. Otherwise it should be `$HOME/.rustup`.
    ///
    /// If this is `None`, it is assumed that rustup is not used.
    pub rustup_home_path: Option<PathBuf>,
}

impl LanguageRustConfiguration {
    fn from_env() -> Self {
        let rustup_home_path = std::env::var_os("TM_RUSTUP_HOME")
            .or_else(|| std::env::var_os("RUSTUP_HOME"))
            .map(Into::into);
        Self { rustup_home_path }
    }
}

/// The Rust language.
#[derive(Debug)]
pub struct LanguageRust {
    /// The configuration of Rust.
    pub config: LanguageRustConfiguration,
}

impl LanguageRust {
    /// Make a new `LanguageRust`.
    pub fn new() -> Self {
        Self {
            config: LanguageRustConfiguration::from_env(),
        }
    }
}

impl Language for LanguageRust {
    fn name(&self) -> &'static str {
        "Rust"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["rs"]
    }

    fn need_compilation(&self) -> bool {
        true
    }

    fn inline_comment_prefix(&self) -> Option<&'static str> {
        Some("//")
    }

    fn compilation_builder(
        &self,
        source: &Path,
        settings: CompilationSettings,
    ) -> Option<Box<dyn CompiledLanguageBuilder + '_>> {
        let mut metadata = SimpleCompiledLanguageBuilder::new(
            self,
            source,
            settings,
            ExecutionCommand::system("rustc"),
        );
        metadata.grader_only();
        // Use a fixed name for the source file, so that the grader can import it.
        metadata.source_name = "source.rs".to_string();
        let binary_name = metadata.binary_name.clone();
        metadata
            .add_arg("-O")
            .add_arg("--cfg")
            .add_arg("EVAL")
            .add_arg("-o")
            .add_arg(binary_name);
        if metadata.settings.list_static {
            metadata
                .add_arg("--target")
                .add_arg("x86_64-unknown-linux-musl");
        }

        let rustup_home_path = self.config.rustup_home_path.clone();
        metadata.callback(move |comp| {
            if let Some(rustup_home_path) = rustup_home_path {
                comp.env(
                    "RUSTUP_HOME",
                    rustup_home_path.to_string_lossy().to_string(),
                );
                comp.limits_mut().add_extra_readable_dir(rustup_home_path);
            }
        });

        Some(Box::new(metadata))
    }
}

#[cfg(test)]
mod tests {
    use speculoos::prelude::*;
    use task_maker_dag::ExecutionDAG;
    use tempfile::TempDir;

    use crate::{
        language::{CompilationSettings, Language},
        GraderMap,
    };

    use super::LanguageRust;

    fn setup() -> TempDir {
        let tempdir = TempDir::new().unwrap();
        let foo = tempdir.path().join("foo.rs");
        std::fs::write(foo, "fn main() {}").unwrap();
        tempdir
    }

    #[test]
    fn test_compilation_args() {
        let tmp = setup();

        let lang = LanguageRust::new();
        let mut builder = lang
            .compilation_builder(&tmp.path().join("foo.rs"), CompilationSettings::default())
            .unwrap();
        let (comp, _exec) = builder.finalize(&mut ExecutionDAG::new()).unwrap();

        let args = comp.args;

        assert_that(&args).contains("source.rs".to_string());
    }

    #[test]
    fn test_compilation_args_with_grader() {
        let tmp = setup();

        let grader_path = tmp.path().join("grader.rs");
        std::fs::write(&grader_path, "mod source;\nfn main() {}").unwrap();

        let lang = LanguageRust::new();
        let mut builder = lang
            .compilation_builder(&tmp.path().join("foo.rs"), CompilationSettings::default())
            .unwrap();

        let graders = GraderMap::new(vec![&grader_path]);
        builder.use_grader(&graders);

        let (comp, _exec) = builder.finalize(&mut ExecutionDAG::new()).unwrap();

        let args = comp.args;

        assert_that(&args).contains("grader.rs".to_string());
        assert_that(&args).does_not_contain("source.rs".to_string());
    }
}
