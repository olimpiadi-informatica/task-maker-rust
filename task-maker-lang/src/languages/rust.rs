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
        let rustup_home_path = std::env::var_os("TM_RUSTUP_HOME").map(Into::into);
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
