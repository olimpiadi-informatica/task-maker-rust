use crate::tools::opt::ClearOpt;

use anyhow::{Context, Error};
use task_maker_format::{find_task, TaskFormat};

pub fn main_clear(opt: ClearOpt) -> Result<(), Error> {
    let task: Box<dyn TaskFormat> = find_task(&opt.task_dir, opt.max_depth, &Default::default())
        .context("Invalid task directory")?;
    task.clean().context("Cannot clear the task directory")?;
    Ok(())
}
