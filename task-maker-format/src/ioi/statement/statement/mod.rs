use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Error};
use serde::{Deserialize, Serialize};

use task_maker_dag::File;
use tex::Tex;
use typst::Typst;

use crate::ioi::statement::asy::AsyFile;
use crate::ioi::{BookletConfig, IOITask};
use crate::ui::UIMessageSender;
use crate::EvaluationData;

use super::Booklet;

mod tex;
mod typst;

/// The configuration of a `Statement`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatementConfig {
    /// The name of the task.
    pub name: String,
    /// The title of the task.
    pub title: String,
    /// The input file of the task, empty for `stdin`.
    pub infile: String,
    /// The output file of the task, empty for `stdout`.
    pub outfile: String,
    /// The time limit of the task.
    pub time_limit: Option<f64>,
    /// The memory limit of the task.
    pub memory_limit: Option<u64>,
    /// The difficulty of the task.
    pub difficulty: Option<u8>,
    /// The level of the syllabus of the task.
    pub syllabus_level: Option<u8>,
}

/// A statement is a `.tex` file with all the other assets included in its directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    /// The configuration of the statement.
    config: StatementConfig,
    /// The path of the `.tex` file.
    pub path: PathBuf,
    /// The content of the `.tex` file, stored here to avoid reading the file many times.
    content: String,
}

/// A typesetting language used for statements
pub trait Language {
    /// The possible extensions of the language
    fn extensions(&self) -> Vec<String>;
    /// Creates the execution for the compilation and adds it to the dag
    fn create_execution(
        self: Box<Self>,
        booklet: &Booklet,
        booklet_name: String,
        eval: &mut EvaluationData,
    ) -> Result<(), Error>;
    /// Builds the source of a single statement file
    fn build_statement_source(&self, statement: &Statement) -> String;
    /// Builds the source of a booklet
    fn build_booklet_source(&self, booklet: &Booklet) -> String;
    /// Emit warnings taken from the compilation stderr
    fn emit_warnings(
        &self,
        booklet_name: PathBuf,
        content: Vec<u8>,
        sender: Arc<Mutex<UIMessageSender>>,
    ) -> Result<(), Error>;
}

/// Returns a valid `impl Language` for the provided extension
pub fn get_language_from_extension(extension: &str) -> Result<Box<dyn Language>, Error> {
    match extension {
        "tex" => Ok(Box::new(Tex {})),
        "typ" => Ok(Box::new(Typst {})),
        _ => bail!("Not a valid extension for statements: {}", extension),
    }
}

impl Statement {
    /// Make a new `Statement` from a `.tex` file and its configuration.
    pub fn new<P: Into<PathBuf>>(path: P, config: StatementConfig) -> Result<Self, Error> {
        let path = path.into();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read statement file from {}", path.display()))?;
        Ok(Statement {
            config,
            path,
            content,
        })
    }

    /// Return a ref to the configuration of the statement.
    pub fn config(&self) -> &StatementConfig {
        &self.config
    }

    /// Build all the dependencies of this statement, returning a vector of (path inside the task
    /// folder, File).
    pub fn build_deps(
        &self,
        eval: &mut EvaluationData,
        booklet_name: &str,
        booklet_config: &BookletConfig,
    ) -> Result<Vec<(PathBuf, File)>, Error> {
        let base_dir = self.path.parent().context("Invalid statement path")?;
        let glob_pattern = base_dir.to_string_lossy().to_string() + "/**/*";
        let logo = booklet_config
            .logo
            .as_ref()
            .and_then(|p| Path::new(p).file_name())
            .map(PathBuf::from);
        let mut deps = vec![];
        for path in glob::glob(&glob_pattern).context("Invalid glob pattern")? {
            let path = path.context("Failed to iterate statement files")?;
            if !path.is_file() {
                continue;
            }
            self.process_possible_dependency(base_dir, &path, &mut deps, eval, booklet_name, &logo)
                .with_context(|| {
                    format!(
                        "Failed to process possible dependency at {}",
                        path.display()
                    )
                })?;
        }
        for path in &[
            Path::new("../gen/limiti.py"),
            Path::new("../gen/constraints.py"),
            Path::new("../gen/limiti.yaml"),
            Path::new("../gen/constraints.yaml"),
            Path::new("../gen/GEN"),
        ] {
            let full_path = base_dir.join(path);
            if !full_path.is_file() {
                continue;
            }
            let file = File::new(format!("Dependency of {} at {:?}", self.config.name, path));
            eval.dag
                .provide_file(file.clone(), &full_path)
                .context("Failed to provide statement dependency")?;
            deps.push((path.file_name().unwrap().into(), file));
        }
        Ok(deps)
    }

    /// Process a possible statement dependency, eventually adding it to the compilation.
    fn process_possible_dependency(
        &self,
        base_dir: &Path,
        path: &Path,
        deps: &mut Vec<(PathBuf, File)>,
        eval: &mut EvaluationData,
        booklet_name: &str,
        logo: &Option<PathBuf>,
    ) -> Result<(), Error> {
        let suffix = path.strip_prefix(base_dir).unwrap();
        let ext = path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if ext.as_str() == "asy" {
            let dest_pdf = suffix.with_extension("pdf");
            let dest_svg = suffix.with_extension("svg");

            let is_dep = self.content.contains(dest_pdf.to_string_lossy().as_ref())
                || self.content.contains(dest_svg.to_string_lossy().as_ref());

            if is_dep {
                let (pdf_file, svg_file) = AsyFile::compile(path, eval, booklet_name)
                    .context("Failed to compile asy file")?;
                deps.push((suffix.with_extension("pdf"), pdf_file));
                deps.push((suffix.with_extension("svg"), svg_file));
            }
        } else {
            if ext == "pdf" {
                // the .pdf file can be the logo of the contest (is present). If it's the logo it is
                // always added.
                let is_logo = match (path.file_name(), &logo) {
                    (Some(name), Some(logo)) => name == logo,
                    _ => false,
                };
                // skip this file if it's not the logo and it's not a valid pdf dependency.
                if !is_logo && !Statement::is_valid_pdf_dependency(path)? {
                    return Ok(());
                }
            }
            let file = File::new(format!(
                "Dependency of {} at {:?}",
                self.config.name, suffix
            ));
            eval.dag
                .provide_file(file.clone(), path)
                .context("Failed to provide statement dependency")?;
            deps.push((suffix.into(), file));
        }
        Ok(())
    }

    /// Check if this pdf file should be considered a valid dependency.
    /// There are some cases where the .pdf should not be considered a dependency:
    /// - The file is the output of an asy compilation: asy will build it from scratch (or use the
    ///   cache)
    /// - The file is the output of a .tex or .typ file (i.e. statement.tex, english.tex, ...): it could be
    ///   the previous output of this statement, do not add it to the sandbox, otherwise the cache
    ///   will miss every time.
    fn is_valid_pdf_dependency(path: &Path) -> Result<bool, Error> {
        // resolve the symlinks
        let path = path
            .canonicalize()
            .with_context(|| format!("Failed to get real path of {}", path.display()))?;
        // ignore .pdf files that have the .asy source
        if path.with_extension("asy").exists() {
            return Ok(false);
        }
        // ignore .pdf files that have the .tex source
        if path.with_extension("tex").exists() {
            return Ok(false);
        }
        // ignore .pdf files that have the .typ source
        if path.with_extension("typ").exists() {
            return Ok(false);
        }
        Ok(true)
    }
}

impl StatementConfig {
    /// Make a new `StatementConfig` from an instance of a `ioi::IOITask`.
    pub fn from_task(task: &IOITask) -> Self {
        StatementConfig {
            name: task.name.clone(),
            title: task.title.clone(),
            infile: task
                .infile
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            outfile: task
                .outfile
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            time_limit: task.time_limit,
            memory_limit: task.memory_limit,
            difficulty: task.difficulty,
            syllabus_level: task.syllabus_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use crate::ioi::{Statement, StatementConfig};
    use crate::EvaluationData;

    #[test]
    fn test_is_valid_pdf_dependency_valid() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        assert!(Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_asy() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        std::fs::write(path.with_extension("asy"), "").unwrap();
        assert!(!Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_tex() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        std::fs::write(path.with_extension("tex"), "").unwrap();
        assert!(!Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_invalid() {
        assert!(Statement::is_valid_pdf_dependency(Path::new("/do/not/exists")).is_err());
    }

    #[test]
    fn test_process_possible_dependency() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("test.tex");
        let logo = tmpdir.path().join("logo.pdf");
        std::fs::write(&path, "lol").unwrap();
        std::fs::write(&logo, "lol").unwrap();
        let statement = Statement::new(&path, StatementConfig::default()).unwrap();
        let logo2 = Some(logo);

        let mut eval = EvaluationData::new(tmpdir.path()).0;
        let in_files = ["logo.pdf", "test.png", "asset.pdf"];
        let not_in_files = ["asy_image.pdf", "tex_file.pdf"];
        std::fs::write(tmpdir.path().join("asy_image.asy"), "").unwrap();
        std::fs::write(tmpdir.path().join("tex_file.tex"), "").unwrap();

        let mut deps = vec![];
        for file in in_files.iter().chain(not_in_files.iter()) {
            let path = tmpdir.path().join(file);
            std::fs::write(&path, "").unwrap();
            statement
                .process_possible_dependency(
                    tmpdir.path(),
                    &path,
                    &mut deps,
                    &mut eval,
                    "name",
                    &logo2,
                )
                .unwrap();
        }
        let deps: Vec<_> = deps
            .into_iter()
            .map(|v| v.0.to_string_lossy().to_string())
            .collect();
        assert_eq!(deps, in_files);
    }
}
