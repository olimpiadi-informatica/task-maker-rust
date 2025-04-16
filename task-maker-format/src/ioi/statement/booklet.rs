use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Error};
use serde::{Deserialize, Serialize};

use crate::ioi::statement::Statement;
use crate::EvaluationData;

use super::get_language_from_extension;

/// Configuration of a `Booklet`, including the setting from the contest configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookletConfig {
    /// The language to use for this booklet, e.g. `"english"`.
    pub language: String,
    /// Whether to show the solutions in the booklet.
    pub show_solutions: bool,
    /// Whether to show the summary of the task.
    pub show_summary: bool,
    /// The font encoding of the tex file.
    pub font_enc: String,
    /// The input encoding of the tex file.
    pub input_enc: String,
    /// The description of the contest.
    pub description: Option<String>,
    /// The location of the contest.
    pub location: Option<String>,
    /// The date of the contest.
    pub date: Option<String>,
    /// The logo of the contest.
    pub logo: Option<String>,
    /// The path to the intro page.
    pub intro_page: Option<PathBuf>,
}

/// A `Booklet` is a pdf file containing the statements of some tasks. It is compiled from a series
/// of `.tex` files defined by `Statement` objects. The compiled pdf file is then copied somewhere.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Booklet {
    /// Configuration of the booklet.
    pub config: BookletConfig,
    /// The list of `Statement`s that are included in this booklet.
    pub statements: Vec<Statement>,
    /// Where to copy the booklet.
    pub dest: PathBuf,
    /// The language extension of the statements
    pub lang: Option<String>,
}

/// Part of the schema of `contest.yaml`, used for extracting the configuration of the booklet.
#[derive(Debug, Deserialize)]
pub struct ContestYAML {
    /// The description of the contest.
    pub description: Option<String>,
    /// The location of the contest.
    pub location: Option<String>,
    /// The date of the contest.
    pub date: Option<String>,
    /// The logo of the contest.
    pub logo: Option<String>,
    /// `true` if the time and memory limits should be put in the booklet.
    #[serde(default)]
    pub show_summary: bool,
    /// Some(relative_path) for a front page for the booklet.
    pub booklet_intro_path: Option<PathBuf>,
    /// The list of the tasks in the contest (in the correct order).
    pub tasks: Vec<String>,
}

impl Booklet {
    /// Make a new `Booklet` using the specified configuration.
    pub fn new<P: Into<PathBuf>>(config: BookletConfig, dest: P) -> Self {
        Booklet {
            config,
            dest: dest.into(),
            statements: Vec::new(),
            lang: None,
        }
    }

    /// Add a `Statement` to this booklet.
    pub fn add_statement(&mut self, statement: Statement) -> Result<(), Error> {
        let Some(statement_lang) = statement.path.extension() else {
            bail!("Statement at {} has no extension", statement.path.display())
        };
        let statement_lang = statement_lang.to_string_lossy().to_string();

        if self
            .lang
            .as_ref()
            .is_some_and(|lang| *lang != statement_lang)
        {
            bail!("Booklet contains statements with two or more different extensions");
        }

        self.lang = Some(statement_lang);

        self.statements.push(statement);
        Ok(())
    }

    /// Build the booklet, eventually coping the final PDF to the specified destination.
    pub fn build(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        let booklet_name = self
            .dest
            .file_name()
            .ok_or_else(|| anyhow!("Invalid destination file {}", self.dest.display()))?
            .to_string_lossy()
            .to_string();

        // If no statement has been added to booklet, there is nothing to do
        let Some(lang) = &self.lang else {
            return Ok(());
        };
        let builder = get_language_from_extension(lang)?;

        builder.create_execution(self, booklet_name, eval)
    }
}

impl BookletConfig {
    /// Build the `BookletConfig` from a contest.
    pub fn from_contest<S: Into<String>, P: Into<PathBuf>>(
        language: S,
        contest_dir: P,
        booklet_solutions: bool,
    ) -> Result<BookletConfig, Error> {
        if let Some(contest_yaml) = Self::contest_yaml(contest_dir) {
            let contest_yaml = contest_yaml?;
            Ok(BookletConfig {
                language: language.into(),
                show_solutions: booklet_solutions,
                show_summary: contest_yaml.show_summary,
                font_enc: "T1".into(),
                input_enc: "utf8".into(),
                description: contest_yaml.description,
                location: contest_yaml.location,
                date: contest_yaml.date,
                logo: contest_yaml.logo,
                intro_page: contest_yaml.booklet_intro_path,
            })
        } else {
            Ok(BookletConfig {
                language: language.into(),
                show_solutions: booklet_solutions,
                show_summary: false,
                font_enc: "T1".into(),
                input_enc: "utf8".into(),
                description: None,
                location: None,
                date: None,
                logo: None,
                intro_page: None,
            })
        }
    }

    /// Find and parse the contest.yaml in the provided contest root.
    pub fn contest_yaml<P: Into<PathBuf>>(contest_dir: P) -> Option<Result<ContestYAML, Error>> {
        let contest_yaml_path = contest_dir.into().join("contest.yaml");
        let parse_yaml = || {
            let file = std::fs::File::open(&contest_yaml_path).with_context(|| {
                format!(
                    "Failed to open contest.yaml at {}",
                    contest_yaml_path.display()
                )
            })?;
            let contest_yaml: ContestYAML =
                serde_yaml::from_reader(file).context("Failed to deserialize contest.yaml")?;
            Ok(contest_yaml)
        };

        if contest_yaml_path.exists() {
            Some(parse_yaml())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::ioi::StatementConfig;

    use super::*;

    fn get_outputs_with_logs(task_root: &Path, copy_logs: bool) -> Result<Vec<PathBuf>, Error> {
        let (mut eval, _recv) = EvaluationData::new(task_root);
        eval.dag.data.config.copy_logs = copy_logs;
        let mut booklet = Booklet::new(BookletConfig::default(), task_root.join("dest.pdf"));
        std::fs::write(task_root.join("text.tex"), "loltex").unwrap();
        let statement =
            Statement::new(task_root.join("text.tex"), StatementConfig::default()).unwrap();
        booklet.add_statement(statement)?;
        booklet.build(&mut eval).unwrap();
        let ret = eval
            .dag
            .file_callbacks()
            .values()
            .filter_map(|f| f.write_to.as_ref())
            .map(|f| f.dest.clone())
            .collect();
        Ok(ret)
    }

    #[test]
    fn test_logs_emitted_with_copy_logs() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let outputs = get_outputs_with_logs(tmpdir.path(), true)
            .expect("Incorrectly found mismatching extensions");
        let stderr_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stderr.log");
        let stdout_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stdout.log");
        assert!(outputs.contains(&stderr_path));
        assert!(outputs.contains(&stdout_path));
    }

    #[test]
    fn test_logs_not_emitted_by_default() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let outputs = get_outputs_with_logs(tmpdir.path(), false)
            .expect("Incorrectly found mismatching extensions");
        let stderr_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stderr.log");
        let stdout_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stdout.log");
        assert!(!outputs.contains(&stderr_path));
        assert!(!outputs.contains(&stdout_path));
    }
}
