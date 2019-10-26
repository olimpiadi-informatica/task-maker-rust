use crate::ioi::statement::data_dir_path;
use crate::ioi::statement::statement::Statement;
use crate::ioi::Tag;
use crate::{bind_exec_callbacks, ui::UIMessage, EvaluationData};
use askama::Template;
use failure::{format_err, Error};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use task_maker_dag::{Execution, ExecutionCommand, File};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
struct ContestYAML {
    /// The description of the contest.
    description: Option<String>,
    /// The location of the contest.
    location: Option<String>,
    /// The date of the contest.
    date: Option<String>,
    /// The logo of the contest.
    logo: Option<String>,
    /// `Some("True")` if the time and memory limits should be put in the booklet.
    show_summary: Option<String>,
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
            .ok_or_else(|| format_err!("Invalid destination file {:?}", self.dest))?
            .to_string_lossy()
            .to_string();
        let mut task_names = self.statements.iter().map(|s| &s.config().name);
        let mut exec = Execution::new(
            "Compilation of the booklet",
            ExecutionCommand::system("latexmk"),
        );
        exec.args(vec![
            "-f",
            "-interaction=nonstopmode",
            "-pdf",
            "booklet.tex",
        ]);
        exec.limits_mut()
            .read_only(false)
            .nproc(1000)
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        exec.tag(Tag::Booklet.into());
        exec.env("TEXINPUTS", format!(".:{}:", task_names.join(":")));
        let output = exec.output("booklet.pdf");

        let source = File::new("Source of the booklet");
        let tex = self.make_tex()?;
        exec.input(&source, "booklet.tex", false);
        eval.dag.provide_content(source, tex.into_bytes());

        for statement in self.statements.iter() {
            let name = &statement.config().name;
            let tex = File::new(format!("Source of statement of {}", name));
            exec.input(&tex, Path::new(&name).join("statement.tex"), false);
            eval.dag.provide_content(tex, statement.tex().into_bytes());
            let base_dir = PathBuf::from(&name);
            for (path, file) in statement.build_deps(eval, &booklet_name, &self.config)? {
                exec.input(file, base_dir.join(path), false);
            }
        }

        // copy all the files from the data/statements directory
        let data_dir = data_dir_path().join("statements");
        let glob_pattern = data_dir.to_string_lossy().to_string() + "/**/*";
        for path in glob::glob(&glob_pattern)? {
            let path = path?;
            if !path.is_file() {
                continue;
            }
            let file = File::new(format!(
                "Booklet template file {:?}",
                path.file_name().expect("Invalid template file")
            ));
            eval.dag.provide_file(file.clone(), &path)?;
            exec.input(file, path.strip_prefix(&data_dir)?, false);
        }

        bind_exec_callbacks!(
            eval,
            exec.uuid,
            |status, name| UIMessage::IOIBooklet { name, status },
            booklet_name
        )?;

        eval.dag.add_execution(exec);
        // latexmk may fail but still produce a good-enough pdf file
        eval.dag.write_file_to_allow_fail(output, &self.dest, false);

        Ok(())
    }

    /// Build the main booklet.tex source file by combining the info from all the statements and
    /// expanding the template.
    fn make_tex(&self) -> Result<String, Error> {
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
        Ok(BookletTemplate {
            language: self.config.language.clone(),
            show_solutions: Booklet::bool_to_tpl_string(
                self.config.show_solutions,
                "showsolutions",
            ),
            show_summary: Booklet::bool_to_tpl_string(self.config.show_summary, "showsummary"),
            font_enc: self.config.font_enc.clone(),
            input_enc: self.config.input_enc.clone(),
            description: self.config.description.clone().unwrap_or_else(String::new),
            location: self.config.location.clone().unwrap_or_else(String::new),
            date: self.config.date.clone().unwrap_or_else(String::new),
            logo: self.config.logo.clone().unwrap_or_else(String::new),
            packages: packages.iter().sorted().join("\n"),
            tasks: tasks.join("\n"),
        }
        .to_string())
    }

    /// Return a string which is `if_true` if `b` is true, otherwise an empty string.
    fn bool_to_tpl_string(b: bool, if_true: &str) -> String {
        if b { if_true } else { "" }.to_string()
    }
}

impl BookletConfig {
    /// Build the `BookletConfig` from a contest.
    pub fn from_contest<S: Into<String>, P: Into<PathBuf>>(
        language: S,
        contest_dir: P,
        booklet_solutions: bool,
    ) -> Result<BookletConfig, Error> {
        let contest_yaml_path = contest_dir.into().join("contest.yaml");
        if contest_yaml_path.exists() {
            let contest_yaml: ContestYAML =
                serde_yaml::from_reader(std::fs::File::open(contest_yaml_path)?)?;
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
}
