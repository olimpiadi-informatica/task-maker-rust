use anyhow::{Context, Error};

use task_maker_format::TaskFormat;

use crate::tools::opt::TaskInfoOpt;

pub fn main_task_info(opt: TaskInfoOpt) -> Result<(), Error> {
    let task: Box<dyn TaskFormat> = opt.find_task.find_task(&Default::default())?;
    let info = task.task_info().context("Cannot produce task info")?;
    if opt.json {
        let json = serde_json::to_string(&info).context("Non-serializable task info")?;
        println!("{}", json);
    } else {
        println!("{:#?} ", info);
    }
    Ok(())
}
