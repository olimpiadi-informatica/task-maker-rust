use super::{Language, Statement};
use crate::bind_exec_callbacks;
use crate::ioi::{Booklet, BOOKLET_PRIORITY};
use crate::ui::{UIMessage, UIMessageSender};
use crate::{EvaluationData, Tag, UISender, DATA_DIR};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error};
use askama::Template;
use itertools::Itertools;
use regex::Regex;
use task_maker_dag::{Execution, ExecutionCommand, File};
use task_maker_diagnostics::Diagnostic;

lazy_static! {
    /// This regex will match all the `\usepackage` inside a latex file.
    static ref USE_PACKAGE_REGEX: Regex = Regex::new(r"\\usepackage.+").expect("Invalid regex");
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
    intro_page: String,
}

#[derive(Debug)]
pub(super) struct Tex;

impl Language for Tex {
    fn extensions(&self) -> Vec<String> {
        vec![String::from("tex")]
    }

    fn create_execution(
        self: Box<Tex>,
        booklet: &Booklet,
        booklet_name: String,
        eval: &mut EvaluationData,
    ) -> Result<(), Error> {
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
        exec.priority(BOOKLET_PRIORITY);
        let output = exec.output("booklet.pdf");

        let source = File::new("Source of the booklet");
        let tex = self.build_booklet_source(booklet);
        exec.input(&source, "booklet.tex", false);
        eval.dag.provide_content(source, tex.into_bytes());

        for statement in booklet.statements.iter() {
            let name = &statement.config().name;
            let tex = File::new(format!("Source of statement of {}", name));
            exec.input(&tex, Path::new(&name).join("statement.tex"), false);
            eval.dag
                .provide_content(tex, self.build_statement_source(statement).into_bytes());
            let base_dir = PathBuf::from(&name);
            let deps = statement
                .build_deps(eval, &booklet_name, &booklet.config)
                .context("Failed to build booklet dependencies")?;

            for (path, file) in &deps {
                exec.input(file, base_dir.join(path), false);
            }

            let deps_paths: Vec<_> = deps.iter().map(|(path, _file)| path.as_path()).collect();

            // provide default limiti.py and constraints.py if needed
            if !deps_paths.contains(&Path::new("limiti.py"))
                && deps_paths.contains(&Path::new("limiti.yaml"))
            {
                let path = DATA_DIR.join("statements/limiti.py");

                let file = File::new("Default limiti.py");
                exec.input(&file, base_dir.join("limiti.py"), false);
                eval.dag.provide_file(file, &path)?;
            }

            if !deps_paths.contains(&Path::new("constraints.py"))
                && deps_paths.contains(&Path::new("constraints.yaml"))
            {
                let path = DATA_DIR.join("statements/constraints.py");

                let file = File::new("Default constraints.py");
                exec.input(&file, base_dir.join("constraints.py"), false);
                eval.dag.provide_file(file, &path)?;
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

        let dest = booklet.dest.file_name().unwrap().to_owned();
        eval.dag.on_execution_done(&exec.uuid, move |res| {
            if let Some(content) = res.stdout {
                self.emit_warnings(PathBuf::from(dest), content, sender)?;
            }
            Ok(())
        });
        eval.dag.add_execution(exec);
        // latexmk may fail but still produce a good-enough pdf file
        eval.dag
            .write_file_to_allow_fail(output, &booklet.dest, false);

        Ok(())
    }

    fn build_statement_source(&self, statement: &Statement) -> String {
        let template = TaskTemplate {
            name: statement.config.name.clone(),
            title: statement.config.title.clone(),
            infile: statement.config.infile.clone(),
            outfile: statement.config.outfile.clone(),
            time_limit: statement
                .config
                .time_limit
                .map(|x| x.to_string())
                .unwrap_or_default(),
            memory_limit: statement
                .config
                .memory_limit
                .map(|x| x.to_string())
                .unwrap_or_default(),
            difficulty: statement
                .config
                .difficulty
                .map(|x| x.to_string())
                .unwrap_or_default(),
            syllabus_level: statement
                .config
                .syllabus_level
                .map(|x| x.to_string())
                .unwrap_or_default(),
            content: USE_PACKAGE_REGEX
                .replace_all(&statement.content, r"% $0")
                .to_string(),
        };
        template.to_string()
    }

    fn build_booklet_source(&self, booklet: &Booklet) -> String {
        let mut packages = HashSet::new();
        let mut tasks = Vec::new();
        for statement in booklet.statements.iter() {
            for package in find_packages(statement) {
                packages.insert(package);
            }
            tasks.push(format!(
                r"\subimport{{./{}/}}{{statement.tex}}",
                statement.config().name
            ));
        }
        BookletTemplate {
            language: booklet.config.language.clone(),
            show_solutions: bool_to_tpl_string(booklet.config.show_solutions, "showsolutions"),
            show_summary: bool_to_tpl_string(booklet.config.show_summary, "showsummary"),
            font_enc: booklet.config.font_enc.clone(),
            input_enc: booklet.config.input_enc.clone(),
            description: booklet.config.description.clone().unwrap_or_default(),
            location: booklet.config.location.clone().unwrap_or_default(),
            date: booklet.config.date.clone().unwrap_or_default(),
            logo: booklet.config.logo.clone().unwrap_or_default(),
            packages: packages.iter().sorted().join("\n"),
            tasks: tasks.join("\n"),
            intro_page: booklet
                .config
                .intro_page
                .clone()
                .map(std::fs::read_to_string)
                .unwrap_or_else(|| Ok(String::new()))
                .unwrap_or_default(),
        }
        .to_string()
    }

    fn emit_warnings(
        &self,
        booklet_name: PathBuf,
        content: Vec<u8>,
        sender: Arc<Mutex<UIMessageSender>>,
    ) -> Result<(), Error> {
        lazy_static! {
            static ref FIND_ERRORS: Regex =
                Regex::new(r"(?ms)^!(?: LaTeX Error:)? ([^\n]+).*?(^l\.\d+)")
                    .expect("Invalid regex");
        }
        // latexmk sometimes emit the same warning more than once
        let mut errors = HashSet::new();
        for cap in FIND_ERRORS.captures_iter(&String::from_utf8_lossy(&content)) {
            let line = cap[2]
                .strip_prefix("l.")
                .and_then(|line| line.parse::<i32>().ok());
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
                    booklet_name.display()
                ))
                .with_note(note),
            )?;
        }
        Ok(())
    }
}

fn find_packages(statement: &Statement) -> Vec<String> {
    let mut packages = Vec::new();
    for package in USE_PACKAGE_REGEX.find_iter(&statement.content) {
        packages.push(package.as_str().to_owned());
    }
    packages
}

/// Return a string which is `if_true` if `b` is true, otherwise an empty string.
fn bool_to_tpl_string(b: bool, if_true: &str) -> String {
    if b { if_true } else { "" }.to_string()
}
