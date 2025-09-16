use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Error};
use clap::Parser;

use task_maker_dag::ProvidedFile;
use task_maker_format::ioi::{make_contest_booklets, Booklet, BookletConfig, IOITask};
use task_maker_format::{find_task, EvaluationConfig};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::context::RuntimeContext;
use crate::ExecutionOpt;

#[derive(Parser, Debug, Clone)]
pub struct ExportBookletOpt {
    /// Include the solutions in the booklet
    #[clap(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Directory of the contest.
    ///
    /// When specified, --task-dir should not be used.
    #[clap(short = 'c', long = "contest-dir")]
    pub contest_dir: Option<PathBuf>,

    /// Directory of the task.
    ///
    /// When specified, --contest-dir should not be used.
    #[clap(short = 't', long = "task-dir")]
    pub task_dir: Vec<PathBuf>,

    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,
}

pub fn main_export_booklet(opt: ExportBookletOpt) -> Result<(), Error> {
    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: opt.booklet_solutions,
        no_statement: false,
        solution_paths: vec![],
        disabled_sanity_checks: vec![],
        seed: None,
        dry_run: true,
    };

    if opt.contest_dir.is_some() && !opt.task_dir.is_empty() {
        bail!("Cannot mix --task-dir and --contest-dir");
    }

    let (mut task, booklets) = if let Some(contest_dir) = opt.contest_dir {
        get_booklets_from_contest_dir(&contest_dir, &eval_config)?
    } else if !opt.task_dir.is_empty() {
        get_booklets_from_task_dirs(&opt.task_dir, &eval_config)?
    } else {
        get_booklets_from_current_dir(&opt, &eval_config)?
    };

    // clean up the task a bit for a cleaner ui
    task.subtasks.clear();

    // setup the configuration and the evaluation metadata
    let context = RuntimeContext::new(task.into(), &ExecutionOpt::default(), |_task, eval| {
        for booklet in booklets {
            booklet.build(eval)?;
        }
        Ok(())
    })?;

    let dag_files = context.eval.dag.data.provided_files;
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut execution_count = 0;
    for (_, execution_group) in context.eval.dag.data.execution_groups {
        for execution in execution_group.executions {
            let mut zip = ZipWriter::new(File::create(format!(
                "booklet_export_{execution_count:0>2}.zip",
            ))?);

            for (name, file) in execution.inputs {
                let file = dag_files
                    .get(&file.file)
                    .ok_or_else(|| anyhow!("File dependency not found."))?;
                let content = match file {
                    ProvidedFile::Content { content, .. } => content.to_owned(),
                    ProvidedFile::LocalFile { local_path, .. } => fs::read(local_path)?,
                };

                zip.start_file(
                    name.to_str().ok_or(anyhow!("Invalid path"))?.to_owned(),
                    options,
                )?;
                zip.write_all(&content)?;
            }

            zip.finish()?;
            execution_count += 1;
        }
    }

    Ok(())
}

fn get_booklets_from_contest_dir(
    contest_dir: &Path,
    eval_config: &EvaluationConfig,
) -> Result<(IOITask, Vec<Booklet>), Error> {
    let contest_yaml = if let Some(contest_yaml) = BookletConfig::contest_yaml(contest_dir) {
        contest_yaml?
    } else {
        bail!("Missing contest.yaml");
    };

    let mut tasks = vec![];
    for task in contest_yaml.tasks {
        let task_dir = contest_dir.join(task);
        tasks.push(task_dir);
    }

    get_booklets_from_task_dirs(&tasks, eval_config)
}

fn get_booklets_from_task_dirs(
    task_dirs: &[PathBuf],
    eval_config: &EvaluationConfig,
) -> Result<(IOITask, Vec<Booklet>), Error> {
    let mut tasks = vec![];
    for path in task_dirs {
        let task = IOITask::new(path, eval_config).with_context(|| {
            format!(
                "Booklet compilation is supported only for IOI tasks for now (task at {})",
                path.display()
            )
        })?;
        tasks.push(task);
    }
    let booklets =
        make_contest_booklets(&tasks, eval_config).context("Failed to get booklet data")?;
    let mut task = IOITask::fake();
    task.title = "Booklet compilation".into();
    task.name = "booklet".into();
    Ok((task, booklets))
}

fn get_booklets_from_current_dir(
    opt: &ExportBookletOpt,
    eval_config: &EvaluationConfig,
) -> Result<(IOITask, Vec<Booklet>), Error> {
    let task = find_task(None, opt.max_depth, eval_config)?;
    let path = task.path();
    let task = IOITask::new(path, eval_config).with_context(|| {
        format!(
            "Booklet compilation is supported only for IOI tasks for now (task at {})",
            path.display()
        )
    })?;

    let booklets = task.booklets.clone();
    Ok((task, booklets))
}
