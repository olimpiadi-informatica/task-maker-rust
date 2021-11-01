use std::path::{Path, PathBuf};

use task_maker_dag::*;

use crate::language::{
    CompilationSettings, CompiledLanguageBuilder, SimpleCompiledLanguageBuilder,
};
use crate::Dependency;
use crate::Language;

/// The Pascal language.
#[derive(Debug)]
pub struct LanguagePascal;

impl LanguagePascal {
    /// Make a new `LanguagePascal`.
    pub fn new() -> LanguagePascal {
        LanguagePascal {}
    }
}

impl Language for LanguagePascal {
    fn name(&self) -> &'static str {
        "Pascal / fpc"
    }

    fn extensions(&self) -> Vec<&'static str> {
        vec!["pas"]
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
            ExecutionCommand::system("fpc"),
        );
        let binary_name = metadata.binary_name.clone();
        metadata
            .add_arg("-dEVAL")
            .add_arg("-Fe/dev/stderr")
            .add_arg("-O2")
            .add_arg("-XS")
            .add_arg(format!("-o{}", binary_name));

        if let Some(fpc_cfg) = find_fpc_cfg() {
            metadata.add_dependency(Dependency {
                file: File::new("fpc configuration"),
                local_path: fpc_cfg,
                sandbox_path: PathBuf::from("fpc.cfg"),
                executable: false,
            });
        }
        Some(Box::new(metadata))
    }
}

/// Search `fpc.cfg` in the local system, following the search rules of
/// https://www.freepascal.org/docs-html/user/usersu10.html
///
/// Returns `None` if `fpc.cfg` cannot be found in the system.
fn find_fpc_cfg() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let path = Path::new(&home).join(".fpc.cfg");
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(config_path) = std::env::var("PPC_CONFIG_PATH") {
        let path = Path::new(&config_path).join("fpc.cfg");
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(fpc) = which::which("fpc") {
        let fpc_parent = fpc.parent().and_then(|p| p.parent());
        if let Some(fpc_parent) = fpc_parent {
            let path = fpc_parent.join("etc/fpc.cfg");
            if path.exists() {
                return Some(path);
            }
        }
    }
    if Path::new("/etc/fpc.cfg").exists() {
        return Some(PathBuf::from("/etc/fpc.cfg"));
    }
    None
}

#[cfg(test)]
mod tests {
    use spectral::prelude::*;
    use tempdir::TempDir;

    use super::*;

    fn setup() -> TempDir {
        let tempdir = TempDir::new("tm-test").unwrap();
        let foo = tempdir.path().join("foo.pas");
        std::fs::write(foo, "some code").unwrap();
        tempdir
    }

    #[test]
    fn test_compilation_args() {
        let tmp = setup();

        let lang = LanguagePascal::new();

        let mut builder = lang
            .compilation_builder(&tmp.path().join("foo.pas"), CompilationSettings::default())
            .unwrap();
        let (comp, _exec) = builder.finalize(&mut ExecutionDAG::new()).unwrap();

        let args = comp.args;
        assert_that!(args).contains("foo.pas".to_string());
    }
}
