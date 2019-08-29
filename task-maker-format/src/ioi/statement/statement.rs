use crate::ioi::statement::asy::AsyFile;
use crate::ioi::Task;
use crate::EvaluationData;
use askama::Template;
use failure::Error;
use regex::Regex;
use std::path::PathBuf;
use task_maker_dag::File;

lazy_static! {
    /// This regex will match all the `\usepackage` inside a latex file.
    static ref USE_PACKAGE_REGEX: Regex = Regex::new(r"\\usepackage.+").unwrap();
}

/// The configuration of a `Statement`.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct Statement {
    /// The configuration of the statement.
    config: StatementConfig,
    /// The path of the `.tex` file.
    path: PathBuf,
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
        let content = std::fs::read_to_string(&path)?;
        Ok(Statement {
            path,
            config,
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
    ) -> Result<Vec<(PathBuf, File)>, Error> {
        let base_dir = self.path.parent().unwrap();
        let glob_pattern = base_dir.to_string_lossy().to_string() + "/**/*";
        let mut deps = vec![];
        for path in glob::glob(&glob_pattern).unwrap() {
            let path = path.unwrap();
            if !path.is_file() {
                continue;
            }
            let suffix = path.strip_prefix(base_dir).unwrap();
            let ext = path
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(String::new);
            match ext.as_str() {
                "asy" => {
                    let dest = suffix.with_extension("pdf");
                    if self.content.contains(dest.to_string_lossy().as_ref()) {
                        let file = AsyFile::compile(&path, eval, booklet_name)?;
                        deps.push((dest, file));
                    }
                }
                _ => {
                    if ext == "pdf" {
                        // resolve the symlinks
                        let path = path.canonicalize()?;
                        // ignore .pdf files that have the .asy source
                        let asy_path = path.with_extension("asy");
                        if asy_path.exists() {
                            continue;
                        }
                        // ignore .pdf files that have the .tex source
                        let tex_path = path.with_extension("tex");
                        if tex_path.exists() {
                            continue;
                        }
                    }
                    let file = File::new(format!(
                        "Dependency of {} at {:?}",
                        self.config.name, suffix
                    ));
                    eval.dag.provide_file(file.clone(), &path)?;
                    deps.push((suffix.into(), file));
                }
            }
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
}

impl StatementConfig {
    /// Make a new `StatementConfig` from an instace of a `ioi::Task`.
    pub fn from_task(task: &Task) -> Self {
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
            difficulty: None,
            syllabus_level: None,
        }
    }
}
