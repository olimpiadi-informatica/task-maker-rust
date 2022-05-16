use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Error};
use clap::{Parser, ValueHint};

use task_maker_format::ioi::{make_context_booklets, Booklet, BookletConfig, IOITask};
use task_maker_format::{find_task, EvaluationConfig};

use crate::context::RuntimeContext;
use crate::{ExecutionOpt, LoggerOpt, StorageOpt, ToolsSandboxRunner, UIOpt};

#[derive(Parser, Debug, Clone)]
pub struct BookletOpt {
    /// Include the solutions in the booklet
    #[clap(long = "booklet-solutions")]
    pub booklet_solutions: bool,

    /// Directory of the context.
    ///
    /// When specified, --task-dir should not be used.
    #[clap(short = 'c', long = "contest-dir", value_hint = ValueHint::DirPath)]
    pub contest_dir: Option<PathBuf>,

    /// Directory of the task.
    ///
    /// When specified, --contest-dir should not be used.
    #[clap(short = 't', long = "task-dir", value_hint = ValueHint::DirPath)]
    pub task_dir: Vec<PathBuf>,

    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,

    #[clap(flatten, next_help_heading = Some("UI"))]
    pub ui: UIOpt,

    #[clap(flatten, next_help_heading = Some("EXECUTION"))]
    pub execution: ExecutionOpt,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: StorageOpt,
}

pub fn main_booklet(mut opt: BookletOpt, logger_opt: LoggerOpt) -> Result<(), Error> {
    opt.ui.disable_if_needed(&logger_opt);
    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: opt.booklet_solutions,
        no_statement: false,
        solution_paths: vec![],
        disabled_sanity_checks: vec![],
        seed: None,
        dry_run: opt.execution.dry_run,
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
    let mut context = RuntimeContext::new(task.into(), &opt.execution, |_task, eval| {
        for booklet in booklets {
            booklet.build(eval)?;
        }
        Ok(())
    })?;
    context.sandbox_runner(ToolsSandboxRunner::default());

    // start the execution
    let executor = context.connect_executor(&opt.execution, &opt.storage)?;
    let executor = executor.start_ui(&opt.ui.ui, |ui, mex| ui.on_message(mex))?;
    executor.execute()?;

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
        make_context_booklets(&tasks, eval_config).context("Failed to get booklet data")?;
    let mut task = IOITask::fake();
    task.title = "Booklet compilation".into();
    task.name = "booklet".into();
    Ok((task, booklets))
}

fn get_booklets_from_current_dir(
    opt: &BookletOpt,
    eval_config: &EvaluationConfig,
) -> Result<(IOITask, Vec<Booklet>), Error> {
    let task = find_task("", opt.max_depth, eval_config)?;
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
