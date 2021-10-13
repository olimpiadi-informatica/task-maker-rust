use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::{ioi, terry, EvaluationConfig, TaskFormat};

/// Search for a valid task directory, starting from base and going _at most_ `max_depth` times up.
pub fn find_task<P: Into<PathBuf>>(
    base: P,
    max_depth: u32,
    eval_config: &EvaluationConfig,
) -> Result<Box<dyn TaskFormat>> {
    let mut base = base.into();
    if !base.is_absolute() {
        base = getcwd().join(base);
    }
    let mut possible_ioi = false;
    let mut possible_terry = false;
    for _ in 0..max_depth {
        possible_ioi = ioi::IOITask::is_valid(&base);
        possible_terry = terry::TerryTask::is_valid(&base);
        if possible_ioi || possible_terry {
            break;
        }
        base = match base.parent() {
            Some(parent) => parent.into(),
            _ => break,
        };
    }
    match (possible_ioi, possible_terry) {
        (true, true) => bail!("Ambiguous task directory, can be either IOI and terry"),
        (false, false) => bail!("No task directory found!"),
        (true, false) => match ioi::IOITask::new(&base, eval_config) {
            Ok(task) => {
                trace!("The task is IOI: {:#?}", task);
                Ok(Box::new(task))
            }
            Err(e) => {
                warn!("Invalid task: {:?}", e);
                Err(e)
            }
        },
        (false, true) => match terry::TerryTask::new(&base, eval_config) {
            Ok(task) => {
                trace!("The task is Terry: {:#?}", task);
                Ok(Box::new(task))
            }
            Err(e) => {
                warn!("Invalid task: {:?}", e);
                Err(e)
            }
        },
    }
}

/// Return the current working directory.
///
/// `std::env::current_dir()` resolves the symlinks of the cwd's hierarchy, `$PWD` is used instead.
fn getcwd() -> PathBuf {
    std::env::var("PWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().expect("Cannot get current working directory"))
}
