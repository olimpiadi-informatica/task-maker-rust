use anyhow::{Context, Error};

use crate::tools::opt::TaskInfoOpt;

pub fn main_task_info(opt: TaskInfoOpt) -> Result<(), Error> {
    let task = opt.find_task.find_task(&Default::default())?;
    let info = task.task_info().context("Cannot produce task info")?;
    if opt.json {
        let json = serde_json::to_string(&info).context("Non-serializable task info")?;
        println!("{}", json);
    } else {
        println!("{:#?} ", info);
    }
    Ok(())
}
