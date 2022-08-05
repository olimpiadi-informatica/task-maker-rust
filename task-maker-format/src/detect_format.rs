use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};

use crate::{ioi, terry, EvaluationConfig, TaskFormat};

/// Search for a valid task directory, starting from base and going _at most_ `max_depth` times up.
pub fn find_task<P: Into<PathBuf>>(
    base: P,
    max_depth: u32,
    eval_config: &EvaluationConfig,
) -> Result<TaskFormat> {
    let mut base = base.into();
    if !base.is_absolute() {
        base = getcwd().join(base);
    }
    let mut fails = vec![];
    for _ in 0..max_depth {
        let mut task = None;
        // try to parse a IOI task
        if ioi::IOITask::is_valid(&base) {
            match ioi::IOITask::new(&base, eval_config) {
                Ok(ioi_task) => task = Some(ioi_task.into()),
                Err(err) => fails.push(("IOI", base.clone(), err)),
            }
        }
        // try to parse a Terry task
        if terry::TerryTask::is_valid(&base) {
            match terry::TerryTask::new(&base, eval_config) {
                Ok(terry_task) => {
                    if task.is_some() {
                        bail!("Ambiguous task directory, can be either IOI and terry")
                    }
                    task = Some(terry_task.into())
                }
                Err(err) => fails.push(("Terry", base.clone(), err)),
            }
        }
        // if a task is found, return it
        if let Some(task) = task {
            return Ok(task);
        }
        // not task found yet, try on the parent folder
        base = match base.parent() {
            Some(parent) => parent.into(),
            _ => break,
        };
    }

    let mut message = "\n".to_string();
    for (format, path, error) in fails {
        let _ = writeln!(
            message,
            "    - Not a valid {} task at {}",
            format,
            path.display()
        );
        error.chain().for_each(|cause| {
            let _ = write!(message, "      Caused by:\n        {}\n", cause);
        });
    }

    Err(anyhow!("{}", message)).context("Cannot find a valid task directory")
}

/// Return the current working directory.
///
/// `std::env::current_dir()` resolves the symlinks of the cwd's hierarchy, `$PWD` is used instead.
fn getcwd() -> PathBuf {
    std::env::var("PWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().expect("Cannot get current working directory"))
}
