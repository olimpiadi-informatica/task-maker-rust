use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use askama::Template;
use regex::Regex;
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::File;

use crate::ioi::statement::asy::AsyFile;
use crate::ioi::{BookletConfig, IOITask};
use crate::EvaluationData;

lazy_static! {
    /// This regex will match all the `\usepackage` inside a latex file.
    static ref USE_PACKAGE_REGEX: Regex = Regex::new(r"\\usepackage.+").expect("Invalid regex");
}

/// The configuration of a `Statement`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, TypeScriptify)]
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
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct Statement {
    /// The configuration of the statement.
    config: StatementConfig,
    /// The path of the `.tex` file.
    pub path: PathBuf,
    /// The content of the `.tex` file, stored here to avoid reading the file many times.
    content: String,
}

/// Template to use to render the `statement.tex` file.
#[derive(Template)]
#[template(path = "task.tex", escape = "none", syntax = "tex")]
struct TaskTemplate {
    name: String,
    title: String,
    infile: String,
    outfile: String,
    time_limit: String,
    memory_limit: String,
    difficulty: String,
    syllabus_level: String,
    content: String,
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
        Ok(deps)
    }

    /// Return the _tex_ source file of the statement, patched with the template.
    pub fn tex(&self) -> String {
        let template = TaskTemplate {
            name: self.config.name.clone(),
            title: self.config.title.clone(),
            infile: self.config.infile.clone(),
            outfile: self.config.outfile.clone(),
            time_limit: self
                .config
                .time_limit
                .map(|x| x.to_string())
                .unwrap_or_else(String::new),
            memory_limit: self
                .config
                .memory_limit
                .map(|x| x.to_string())
                .unwrap_or_else(String::new),
            difficulty: self
                .config
                .difficulty
                .map(|x| x.to_string())
                .unwrap_or_else(String::new),
            syllabus_level: self
                .config
                .syllabus_level
                .map(|x| x.to_string())
                .unwrap_or_else(String::new),
            content: USE_PACKAGE_REGEX
                .replace_all(&self.content, r"% $0")
                .to_string(),
        };
        template.to_string()
    }

    /// Return a list of all the `\usepackage` used by the statement.
    pub fn packages(&self) -> Vec<String> {
        let mut packages = Vec::new();
        for package in USE_PACKAGE_REGEX.find_iter(&self.content) {
            packages.push(package.as_str().to_owned());
        }
        packages
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
            .unwrap_or_else(String::new);
        if ext.as_str() == "asy" {
            let dest = suffix.with_extension("pdf");
            if self.content.contains(dest.to_string_lossy().as_ref()) {
                let file = AsyFile::compile(&path, eval, booklet_name)
                    .context("Failed to compile asy file")?;
                deps.push((dest, file));
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
                .provide_file(file.clone(), &path)
                .context("Failed to provide statement dependency")?;
            deps.push((suffix.into(), file));
        }
        Ok(())
    }

    /// Check if this pdf file should be considered a valid dependency.
    /// There are some cases where the .pdf should not be considered a dependency:
    /// - The file is the output of an asy compilation: asy will build it from scratch (or use the
    ///   cache)
    /// - The file is the output of a .tex file (i.e. statement.tex, english.tex, ...): it could be
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
        Ok(true)
    }
}

impl StatementConfig {
    /// Make a new `StatementConfig` from an instace of a `ioi::Task`.
    pub fn from_task(task: &IOITask) -> Self {
        StatementConfig {
            name: task.name.clone(),
            title: task.title.clone(),
            infile: task
                .infile
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(String::new),
            outfile: task
                .outfile
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(String::new),
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

    use tempdir::TempDir;

    use crate::ioi::{Statement, StatementConfig};
    use crate::EvaluationData;

    #[test]
    fn test_is_valid_pdf_dependency_valid() {
        let tmpdir = TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        assert!(Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_asy() {
        let tmpdir = TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        std::fs::write(&path.with_extension("asy"), "").unwrap();
        assert!(!Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_tex() {
        let tmpdir = TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("test.pdf");
        std::fs::write(&path, "").unwrap();
        std::fs::write(&path.with_extension("tex"), "").unwrap();
        assert!(!Statement::is_valid_pdf_dependency(&path).unwrap());
    }

    #[test]
    fn test_is_valid_pdf_dependency_invalid() {
        assert!(Statement::is_valid_pdf_dependency(Path::new("/do/not/exists")).is_err());
    }

    #[test]
    fn test_process_possible_dependency() {
        let tmpdir = TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("test.tex");
        let logo = tmpdir.path().join("logo.pdf");
        std::fs::write(&path, "lol").unwrap();
        std::fs::write(&logo, "lol").unwrap();
        let statement = Statement::new(&path, StatementConfig::default()).unwrap();
        let logo2 = Some(logo);

        let mut eval = EvaluationData::new(tmpdir.path()).0;
        let in_files = vec!["logo.pdf", "test.png", "asset.pdf"];
        let not_in_files = vec!["asy_image.pdf", "tex_file.pdf"];
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
