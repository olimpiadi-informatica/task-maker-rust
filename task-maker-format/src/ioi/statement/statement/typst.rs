use super::{Language, Statement};
use crate::ioi::{Booklet, BOOKLET_PRIORITY};
use crate::ui::{UIMessage, UIMessageSender};
use crate::{bind_exec_callbacks, UISender};
use crate::{EvaluationData, Tag};

use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error};
use itertools::Itertools;
use task_maker_dag::{Execution, ExecutionCommand, File};
use task_maker_diagnostics::Diagnostic;

#[derive(Debug)]
pub(super) struct Typst;

impl Language for Typst {
    fn extensions(&self) -> Vec<String> {
        vec![String::from("typ")]
    }

    fn create_execution(
        self: Box<Typst>,
        booklet: &Booklet,
        booklet_name: String,
        eval: &mut EvaluationData,
    ) -> Result<(), Error> {
        let mut exec = Execution::new(
            "Compilation of the booklet",
            ExecutionCommand::system("typst"),
        );

        exec.args(vec![
            "compile",
            "booklet.typ",
            "--input",
            "gen_gen=GEN",
            "--input",
            "constraints_yaml=constraints.yaml",
            "--input",
            "contest_yaml=../../contest.yaml",
            "--package-cache-path",
            "typst-cache",
        ]);

        exec.limits_mut()
            .read_only(false)
            .allow_multiprocess()
            .mount_tmpfs(true)
            .mount_proc(true)
            .add_extra_readable_dir("/etc");

        exec.tag(Tag::Booklet.into());
        exec.priority(BOOKLET_PRIORITY);
        let output = exec.output("booklet.pdf");

        let source = File::new("Source of the booklet");
        let typst = self.build_booklet_source(booklet);
        exec.input(&source, "booklet.typ", false);
        eval.dag.provide_content(source, typst.into_bytes());

        let contest_yaml = File::new("Contest yaml");
        exec.input(&contest_yaml, "contest.yaml", false);
        eval.dag.provide_content(
            contest_yaml,
            serde_yaml::to_string(&booklet.config)?.into_bytes(),
        );

        for statement in booklet.statements.iter() {
            let name = &statement.config().name;
            let typst = File::new(format!("Source of statement of {name}"));
            exec.input(
                &typst,
                Path::new(&name).join("statement/statement.typ"),
                false,
            );
            eval.dag
                .provide_content(typst, self.build_statement_source(statement).into_bytes());

            let task_yaml = File::new(format!("task.yaml for {name}"));
            exec.input(&task_yaml, Path::new(&name).join("task.yaml"), false);
            eval.dag.provide_content(
                task_yaml,
                serde_yaml::to_string(statement.config())?.into_bytes(),
            );

            let base_dir = PathBuf::from(&name).join("statement");
            let deps = statement
                .build_deps(eval, &booklet_name, &booklet.config)
                .context("Failed to build booklet dependencies")?;

            for (path, file) in &deps {
                let path = if path == Path::new("limiti.yaml") {
                    Path::new("constraints.yaml")
                } else {
                    path
                };

                exec.input(file, base_dir.join(path), false);
            }
        }

        // Copy the intro page if needed
        if let Some(intro_page) = &booklet.config.intro_page {
            let intro = File::new("Intro page for booklet");
            exec.input(&intro, "intro_page.typ", false);
            eval.dag.provide_file(intro, intro_page)?;
        }

        // Copy the typst cache to provide packages
        let cache_dir = match env::var("XDG_CACHE_HOME") {
            Ok(cache) => Path::new(&cache).join("typst/packages"),
            Err(_) => Path::new(&env::var("HOME")?).join(".cache/typst/packages"),
        };
        let glob_pattern = cache_dir.to_string_lossy().to_string() + "/**/*";
        for path in glob::glob(&glob_pattern).context("Invalid glob pattern")? {
            let path = path.context("Failed to iterate with glob")?;
            if !path.is_file() {
                continue;
            }
            let file = File::new(format!("Typst package file at {:?}", path.display(),));
            eval.dag
                .provide_file(file.clone(), &path)
                .context("Failed to provide typst package file")?;
            exec.input(
                file,
                Path::new("typst-cache").join(path.strip_prefix(&cache_dir)?),
                false,
            );
        }

        bind_exec_callbacks!(
            eval,
            exec.uuid,
            |status, name| UIMessage::IOIBooklet { name, status },
            booklet_name
        )?;
        if eval.dag.data.config.copy_logs {
            let log_dir = eval.task_root.join("bin/logs/booklets");
            let stderr_dest = log_dir.join(format!("{booklet_name}.stderr.log"));
            let stdout_dest = log_dir.join(format!("{booklet_name}.stdout.log"));
            eval.dag
                .write_file_to_allow_fail(exec.stderr(), stderr_dest, false);
            eval.dag
                .write_file_to_allow_fail(exec.stdout(), stdout_dest, false);
        }
        let sender = eval.sender.clone();
        exec.capture_stderr(1024 * 1024 * 1024);

        let dest = booklet.dest.file_name().unwrap().to_owned();
        eval.dag.on_execution_done(&exec.uuid, move |res| {
            if let Some(content) = res.stderr {
                self.emit_warnings(PathBuf::from(dest), content, sender)?;
            }
            Ok(())
        });
        eval.dag.add_execution(exec);

        eval.dag.write_file_to(output, &booklet.dest, false);

        Ok(())
    }

    fn build_statement_source(&self, statement: &Statement) -> String {
        statement.content.clone()
    }

    fn build_booklet_source(&self, booklet: &Booklet) -> String {
        let statements = booklet
            .statements
            .iter()
            .map(|statement| {
                format!(
                    "#include \"{}/statement/statement.typ\"",
                    statement.config().name
                )
            })
            .join("\n");

        if booklet.config.intro_page.is_some() {
            format!("#include \"intro_page.typ\"\n{statements}")
        } else {
            statements
        }
    }

    fn emit_warnings(
        &self,
        booklet_name: PathBuf,
        content: Vec<u8>,
        sender: Arc<Mutex<UIMessageSender>>,
    ) -> Result<(), Error> {
        if !content.is_empty() {
            let mut buf = String::new();
            content.as_slice().read_to_string(&mut buf)?;

            sender.add_diagnostic(
                Diagnostic::warning(format!(
                    "The compilation of the booklet at {} produced the following errors or warnings",
                    booklet_name.display()
                )).with_note(&buf).with_help("Use --copy-logs to save the compilation stderr")
            )?;

            // if it seems like typst tried to download a package we emit an error
            if buf.contains("error: failed to download package") {
                sender.add_diagnostic(
                    Diagnostic::error("Typst tried to download a package, but couldn't")
                        .with_note("This is because the compilation of the booklet is sandboxed")
                        .with_help(
                            "Try compiling manually once to store the package in the offline cache",
                        ),
                )?;
            }
        }
        Ok(())
    }
}
