use std::fs::{create_dir, copy};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Error};
use clap::Parser;

use task_maker_format::ioi::{Booklet, BookletConfig, IOITask, IOITaskInfo};
use task_maker_format::{find_task, EvaluationConfig, TaskInfo};

use crate::context::RuntimeContext;
use crate::{ExecutionOpt, LoggerOpt, StorageOpt, ToolsSandboxRunner, UIOpt};

#[derive(Parser, Debug, Clone)]
pub struct CopyCompetitionFilesOpt {
    /// Directory of the context.
    ///
    /// When not specified, . is assumed.
    #[clap(short = 'c', long = "contest-dir", default_value = ".")]
    pub contest_dir: PathBuf,

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

pub fn copy_competition_files_main(mut opt: CopyCompetitionFilesOpt, logger_opt: LoggerOpt) -> Result<(), Error> {
    opt.ui.disable_if_needed(&logger_opt);
    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: false,
        no_statement: false,
        solution_paths: vec![],
        disabled_sanity_checks: vec![],
        seed: None,
        dry_run: opt.execution.dry_run,
    };

    // create folder for competition files
    let competition_files_dir = opt.contest_dir.join("competition-files");
    if !competition_files_dir.exists() {
        create_dir(&competition_files_dir)?;
    }

    for (mut task, booklets) in get_tasks_from_contest_dir(&opt.contest_dir, &opt, &eval_config)? {
        // clean up the task a bit for a cleaner ui
        task.subtasks.clear();

        // setup the configuration and the evaluation metadata
        let mut context = RuntimeContext::new(task.clone().into(), &opt.execution, |_task, eval| {
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

        let TaskInfo::IOI(task_info) = task.task_info()? else {
            bail!("Competition folder creation is supported only for IOI tasks now");
        };

        copy_files(task_info, &opt.contest_dir.join(&task.name), &competition_files_dir.join(&task.name))?;
    }

    Ok(())
}

fn copy_files(task_info: IOITaskInfo, task_path: &Path, files_path: &Path) -> Result<(), Error> {
    // create problem directory
    if !files_path.exists() {
        create_dir(&files_path)?;
    }
    
    // copy statements
    for statement in task_info.statements {
        let statement_path = task_path.join(&statement.path);
        let target_path = files_path.join(&statement.path.file_name().unwrap());
        
        copy(statement_path, target_path)?;
    }

    // copy attachments
    for attachment in task_info.attachments {
        let attachment_path = task_path.join(&attachment.path);
        let target_path = files_path.join(&attachment.path.file_name().unwrap());
        
        copy(attachment_path, target_path)?;
    }

    Ok(())
}

fn get_tasks_from_contest_dir(
    contest_dir: &Path,
    opt: &CopyCompetitionFilesOpt,
    eval_config: &EvaluationConfig,
) -> Result<Vec<(IOITask, Vec<Booklet>)>, Error> {
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

    tasks.iter().map(|task| get_task_from_task_dir(task, opt, eval_config)).collect()
}

fn get_task_from_task_dir(
    path: &Path,
    opt: &CopyCompetitionFilesOpt,
    eval_config: &EvaluationConfig,
) -> Result<(IOITask, Vec<Booklet>), Error> {
    let task = find_task(Some(path.into()), opt.max_depth, eval_config)?;
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
