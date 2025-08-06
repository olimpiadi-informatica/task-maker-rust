use anyhow::{Context, Error};
use clap::Parser;

use crate::FindTaskOpt;

#[derive(Parser, Debug, Clone)]
pub struct TaskInfoOpt {
    #[clap(flatten, next_help_heading = Some("TASK SEARCH"))]
    pub find_task: FindTaskOpt,
    /// Produce JSON output.
    #[clap(long, short)]
    pub json: bool,
}

pub fn main_task_info(opt: TaskInfoOpt) -> Result<(), Error> {
    let task = opt.find_task.find_task(&Default::default())?;
    let info = task.task_info().context("Cannot produce task info")?;
    if opt.json {
        let json = serde_json::to_string(&info).context("Non-serializable task info")?;
        println!("{json}");
    } else {
        println!("{info:#?} ");
    }
    Ok(())
}
