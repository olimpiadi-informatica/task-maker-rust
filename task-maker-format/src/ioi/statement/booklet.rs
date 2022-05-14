use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Error};
use askama::Template;
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use task_maker_dag::{Execution, ExecutionCommand, File};
use task_maker_diagnostics::Diagnostic;

use crate::ioi::statement::statement::Statement;
use crate::ui::UIMessageSender;
use crate::{bind_exec_callbacks, ui::UIMessage, EvaluationData, Tag, UISender, DATA_DIR};

/// Configuration of a `Booklet`, including the setting from the contest configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, TypeScriptify)]
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
}

/// Template to use to render the `booklet.tex` file.
#[derive(Template)]
#[template(path = "booklet.tex", escape = "none", syntax = "tex")]
pub struct BookletTemplate {
    language: String,
    show_solutions: String,
    show_summary: String,
    font_enc: String,
    input_enc: String,
    description: String,
    location: String,
    date: String,
    logo: String,
    packages: String,
    tasks: String,
}

/// A `Booklet` is a pdf file containing the statements of some tasks. It is compiled from a series
/// of `.tex` files defined by `Statement` objects. The compiled pdf file is then copied somewhere.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct Booklet {
    /// Configuration of the booklet.
    pub config: BookletConfig,
    /// The list of `Statement`s that are included in this booklet.
    pub statements: Vec<Statement>,
    /// Where to copy the booklet.
    pub dest: PathBuf,
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
    /// `Some("True")` if the time and memory limits should be put in the booklet.
    pub show_summary: Option<String>,
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
        }
    }

    /// Add a `Statement` to this booklet.
    pub fn add_statement(&mut self, statement: Statement) {
        self.statements.push(statement);
    }

    /// Build the booklet, eventually coping the final PDF to the specified destination.
    pub fn build(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        let booklet_name = self
            .dest
            .file_name()
            .ok_or_else(|| anyhow!("Invalid destination file {}", self.dest.display()))?
            .to_string_lossy()
            .to_string();
        let mut task_names = self.statements.iter().map(|s| &s.config().name);
        let mut exec = Execution::new(
            "Compilation of the booklet",
            ExecutionCommand::system("latexmk"),
        );
        exec.args(vec![
            "-shell-escape",
            "-f",
            "-interaction=nonstopmode",
            "-pdf",
            "booklet.tex",
        ]);
        exec.limits_mut()
            .read_only(false)
            .allow_multiprocess()
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        exec.tag(Tag::Booklet.into());
        exec.env("TEXINPUTS", format!(".:{}:", task_names.join(":")));
        let output = exec.output("booklet.pdf");

        let source = File::new("Source of the booklet");
        let tex = self.make_tex();
        exec.input(&source, "booklet.tex", false);
        eval.dag.provide_content(source, tex.into_bytes());

        for statement in self.statements.iter() {
            let name = &statement.config().name;
            let tex = File::new(format!("Source of statement of {}", name));
            exec.input(&tex, Path::new(&name).join("statement.tex"), false);
            eval.dag.provide_content(tex, statement.tex().into_bytes());
            let base_dir = PathBuf::from(&name);
            let deps = statement
                .build_deps(eval, &booklet_name, &self.config)
                .context("Failed to build booklet dependencies")?;
            for (path, file) in deps {
                exec.input(file, base_dir.join(path), false);
            }
        }

        // copy all the files from the data/statements directory
        let data_dir = DATA_DIR.join("statements");
        let glob_pattern = data_dir.to_string_lossy().to_string() + "/**/*";
        for path in glob::glob(&glob_pattern).context("Invalid glob pattern")? {
            let path = path.context("Failed to iterate with glob")?;
            if !path.is_file() {
                continue;
            }
            let file = File::new(format!(
                "Booklet template file {:?}",
                path.file_name().context("Invalid template file")?
            ));
            eval.dag
                .provide_file(file.clone(), &path)
                .context("Failed to provide statement file")?;
            exec.input(file, path.strip_prefix(&data_dir)?, false);
        }

        bind_exec_callbacks!(
            eval,
            exec.uuid,
            |status, name| UIMessage::IOIBooklet { name, status },
            booklet_name
        )?;
        if eval.dag.data.config.copy_logs {
            let log_dir = eval.task_root.join("bin/logs/booklets");
            let stderr_dest = log_dir.join(format!("{}.stderr.log", booklet_name));
            let stdout_dest = log_dir.join(format!("{}.stdout.log", booklet_name));
            eval.dag
                .write_file_to_allow_fail(exec.stderr(), stderr_dest, false);
            eval.dag
                .write_file_to_allow_fail(exec.stdout(), stdout_dest, false);
        }
        let sender = eval.sender.clone();
        exec.capture_stdout(1024 * 1024 * 1024);
        let dest = self.dest.file_name().unwrap().to_owned();
        eval.dag.on_execution_done(&exec.uuid, move |res| {
            if let Some(content) = &res.stdout {
                Booklet::emit_warnings(dest, content, sender)?;
            }
            Ok(())
        });
        eval.dag.add_execution(exec);
        // latexmk may fail but still produce a good-enough pdf file
        eval.dag.write_file_to_allow_fail(output, &self.dest, false);

        Ok(())
    }

    /// Build the main booklet.tex source file by combining the info from all the statements and
    /// expanding the template.
    fn make_tex(&self) -> String {
        let mut packages = HashSet::new();
        let mut tasks = Vec::new();
        for statement in self.statements.iter() {
            for package in statement.packages() {
                packages.insert(package);
            }
            tasks.push(format!(
                r"\input{{{}/statement.tex}}",
                statement.config().name
            ));
        }
        BookletTemplate {
            language: self.config.language.clone(),
            show_solutions: Booklet::bool_to_tpl_string(
                self.config.show_solutions,
                "showsolutions",
            ),
            show_summary: Booklet::bool_to_tpl_string(self.config.show_summary, "showsummary"),
            font_enc: self.config.font_enc.clone(),
            input_enc: self.config.input_enc.clone(),
            description: self.config.description.clone().unwrap_or_default(),
            location: self.config.location.clone().unwrap_or_default(),
            date: self.config.date.clone().unwrap_or_default(),
            logo: self.config.logo.clone().unwrap_or_default(),
            packages: packages.iter().sorted().join("\n"),
            tasks: tasks.join("\n"),
        }
        .to_string()
    }

    /// Return a string which is `if_true` if `b` is true, otherwise an empty string.
    fn bool_to_tpl_string(b: bool, if_true: &str) -> String {
        if b { if_true } else { "" }.to_string()
    }

    /// Given the content of the log from latexmk, extract the errors and emit them as warnings.
    fn emit_warnings(
        booklet_name: impl AsRef<Path>,
        content: &[u8],
        sender: Arc<Mutex<UIMessageSender>>,
    ) -> Result<(), Error> {
        lazy_static! {
            static ref FIND_ERRORS: Regex =
                Regex::new(r"(?ms)^!(?: LaTeX Error:)? ([^\n]+).*?(^l\.\d+)")
                    .expect("Invalid regex");
        }
        // latexmk sometimes emit the same warning more than once
        let mut errors = HashSet::new();
        for cap in FIND_ERRORS.captures_iter(&String::from_utf8_lossy(content)) {
            let line = cap[2][2..].parse::<i32>().ok();
            errors.insert((line, cap[1].to_string()));
        }
        if !errors.is_empty() {
            let note = errors
                .into_iter()
                .sorted()
                .map(|(line, error)| {
                    if let Some(line) = line {
                        format!("Line {}: {}", line, error)
                    } else {
                        error
                    }
                })
                .join("\n");
            sender.add_diagnostic(
                Diagnostic::warning(format!(
                    "Found Latex errors while compiling the booklet {}",
                    booklet_name.as_ref().display()
                ))
                .with_note(note),
            )?;
        }
        Ok(())
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
                show_summary: contest_yaml.show_summary == Some("True".to_string()),
                font_enc: "T1".into(),
                input_enc: "utf8".into(),
                description: contest_yaml.description,
                location: contest_yaml.location,
                date: contest_yaml.date,
                logo: contest_yaml.logo,
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
    use crate::ioi::StatementConfig;

    use super::*;

    fn get_outputs_with_logs(task_root: &Path, copy_logs: bool) -> Vec<PathBuf> {
        let (mut eval, _recv) = EvaluationData::new(task_root);
        eval.dag.data.config.copy_logs = copy_logs;
        let mut booklet = Booklet::new(BookletConfig::default(), task_root.join("dest.pdf"));
        std::fs::write(task_root.join("text.tex"), "loltex").unwrap();
        let statement =
            Statement::new(task_root.join("text.tex"), StatementConfig::default()).unwrap();
        booklet.add_statement(statement);
        booklet.build(&mut eval).unwrap();
        eval.dag
            .file_callbacks()
            .values()
            .filter_map(|f| f.write_to.as_ref())
            .map(|f| f.dest.clone())
            .collect()
    }

    #[test]
    fn test_logs_emitted_with_copy_logs() {
        let tmpdir = tempdir::TempDir::new("tm-tests").unwrap();
        let outputs = get_outputs_with_logs(tmpdir.path(), true);
        let stderr_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stderr.log");
        let stdout_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stdout.log");
        assert!(outputs.contains(&stderr_path));
        assert!(outputs.contains(&stdout_path));
    }

    #[test]
    fn test_logs_not_emitted_by_default() {
        let tmpdir = tempdir::TempDir::new("tm-tests").unwrap();
        let outputs = get_outputs_with_logs(tmpdir.path(), false);
        let stderr_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stderr.log");
        let stdout_path = tmpdir.path().join("bin/logs/booklets/dest.pdf.stdout.log");
        assert!(!outputs.contains(&stderr_path));
        assert!(!outputs.contains(&stdout_path));
    }
}
