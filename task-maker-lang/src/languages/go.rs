use std::path::Path;

use task_maker_dag::*;

use crate::language::{
    CompilationSettings, CompiledLanguageBuilder, Language, SimpleCompiledLanguageBuilder,
};

/// The Go language.
#[derive(Debug)]
pub struct LanguageGo;

impl LanguageGo {
    /// Make a new LanguageGo
    pub fn new() -> LanguageGo {
        LanguageGo
    }
}

impl Language for LanguageGo {
    fn name(&self) -> &'static str {
        "Go"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["go"]
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
            ExecutionCommand::system("go"),
        );
        let binary_name = metadata.binary_name.clone();
        metadata.add_arg("build").add_arg("-o").add_arg(binary_name);

        metadata.callback(|comp| {
            comp.env("GOCACHE", "/tmp");
            comp.env("GO111MODULE", "off");
            comp.env("CGO_ENABLED", "0");
            #[cfg(target_os = "linux")]
            match std::env::consts::ARCH {
                "x86_64" => {
                    comp.env("GOARCH", "386");
                }
                "aarch64" => {
                    comp.env("GOARCH", "arm");
                }
                _ => {}
            }
        });

        Some(Box::new(metadata))
    }

    fn custom_limits(&self, limits: &mut ExecutionLimits) {
        limits.permissive = true;
    }
}
