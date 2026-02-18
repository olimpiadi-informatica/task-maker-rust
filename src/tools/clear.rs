use anyhow::{Context, Error};
use clap::Parser;

use crate::FindTaskOpt;

#[derive(Parser, Debug)]
pub struct ClearOpt {
    #[clap(flatten, next_help_heading = Some("TASK SEARCH"))]
    pub find_task: FindTaskOpt,
}

pub fn main_clear(opt: ClearOpt) -> Result<(), Error> {
    let task = opt.find_task.find_task(&Default::default())?;
    task.clean().context("Cannot clear the task directory")?;
    Ok(())
}
