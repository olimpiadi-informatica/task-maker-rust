use anyhow::{Context, Error};

use task_maker_format::TaskFormat;

use crate::tools::opt::ClearOpt;

pub fn main_clear(opt: ClearOpt) -> Result<(), Error> {
    let task: Box<dyn TaskFormat> = opt.find_task.find_task(&Default::default())?;
    task.clean().context("Cannot clear the task directory")?;
    Ok(())
}
