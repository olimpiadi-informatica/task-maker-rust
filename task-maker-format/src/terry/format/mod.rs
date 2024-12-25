use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error};
use serde::{Deserialize, Serialize};

use crate::terry::dag::{Checker, InputGenerator, InputValidator};
use crate::terry::sanity_checks::get_sanity_checks;
use crate::terry::statement::Statement;
use crate::terry::TerryTask;
use crate::{find_source_file, EvaluationConfig, SourceFile, WriteBinTo};

lazy_static! {
    /// The extension suffix for the current platform.
    static ref EXE_EXTENSION: String =
        format!("{}.{}", std::env::consts::OS, std::env::consts::ARCH);
}

/// Deserialized data from the task.yaml of a IOI format task.
#[derive(Debug, Serialize, Deserialize)]
struct TaskYAML {
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub description: String,
    /// The maximum score for this task.
    pub max_score: f64,
}

/// Given a path to a task in the Terry format, try to parse the task inside of it.
pub fn parse_task<P: AsRef<Path>>(
    task_dir: P,
    eval_config: &EvaluationConfig,
) -> Result<TerryTask, Error> {
    let task_dir = task_dir.as_ref();
    let yaml: TaskYAML = serde_yaml::from_reader(fs::File::open(task_dir.join("task.yaml"))?)?;

    let statement = get_statement_template(task_dir)?;
    let generator = get_manager(task_dir, "generator")?
        .map(InputGenerator::new)
        .ok_or_else(|| anyhow!("No generator found in managers/"))?;
    let validator = get_manager(task_dir, "validator")?.map(InputValidator::new);
    let checker = get_manager(task_dir, "checker")?
        .map(Checker::new)
        .ok_or_else(|| anyhow!("No checker found in managers/"))?;
    let official_solution = get_manager(task_dir, "solution")?;

    Ok(TerryTask {
        path: task_dir.into(),
        name: yaml.name,
        description: yaml.description,
        max_score: yaml.max_score,
        statement,
        generator,
        validator,
        checker,
        official_solution,
        sanity_checks: Arc::new(get_sanity_checks(
            &eval_config
                .disabled_sanity_checks
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )),
    })
}

fn get_statement_template(task_dir: &Path) -> Result<Option<Statement>, Error> {
    let path = task_dir.join("statement/statement.in.md");

    if !path.exists() {
        return Ok(None);
    }

    let subtasks_path = task_dir.join("managers/subtasks.yaml");
    let subtasks = if subtasks_path.exists() {
        Some(subtasks_path)
    } else {
        None
    };

    Ok(Some(Statement { path, subtasks }))
}

/// Search the specified manager in the managers/ folder of the task, returning the `SourceFile` if
/// found, `None` otherwise.
fn get_manager(task_dir: &Path, manager: &str) -> Result<Option<Arc<SourceFile>>, Error> {
    let mut managers = find_source_file(
        task_dir,
        vec![&format!("managers/{}.*", manager)],
        task_dir,
        "Terry manager at",
        None,
        WriteBinTo::path(format!("managers/{}.{}", manager, *EXE_EXTENSION)),
    );
    if managers.len() > 1 {
        let paths = managers.iter().map(|s| s.name()).collect::<Vec<_>>();
        bail!("Multiple managers found: {:?}", paths);
    }
    Ok(managers.pop().map(|mut s| {
        s.copy_exe(); // The managers are always copied.
        s.link_static(); // Make sure the managers are statically linked.
        Arc::new(s)
    }))
}
