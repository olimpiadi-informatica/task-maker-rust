use anyhow::{Context, Error};

use crate::tools::opt::ClearOpt;

pub fn main_clear(opt: ClearOpt) -> Result<(), Error> {
    let task = opt.find_task.find_task(&Default::default())?;
    task.clean().context("Cannot clear the task directory")?;
    Ok(())
}
