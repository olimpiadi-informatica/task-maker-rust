use std::fs;

use anyhow::{anyhow, Context, Error};
use clap::Parser;

use crate::LoggerOpt;
use task_maker_format::terry::{StatementSubtask, TerryTask};
use task_maker_format::{find_task, EvaluationConfig};

#[derive(Parser, Debug, Clone)]
pub struct TerryStatementOpt {
    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,
}

pub fn main_terry_statement(opt: TerryStatementOpt, _logger_opt: LoggerOpt) -> Result<(), Error> {
    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: false,
        no_statement: false,
        solution_paths: vec![],
        disabled_sanity_checks: vec![],
        seed: None,
        dry_run: false,
    };

    let task = find_task(None, opt.max_depth, &eval_config)?;
    let path = task.path();
    let task = TerryTask::new(path, &eval_config)
        .with_context(|| format!("There is not Terry task at {}", path.display()))?;

    let Some(statement) = task.statement else {
        return Ok(());
    };

    let content = fs::read_to_string(statement.path)?;

    let new_content = if content.contains("<subtasks-recap/>") {
        let subtasks = statement
            .subtasks
            .ok_or(anyhow!("No subtasks.yaml file."))?;
        let subtasks = generate_md_table(&subtasks);

        content.replace("<subtasks-recap/>", &subtasks)
    } else {
        content
    };

    fs::write(path.join("statement/statement.md"), new_content)?;

    Ok(())
}

fn generate_md_table(subtasks: &[StatementSubtask]) -> String {
    let mut table = String::from("| | Limiti | Punti |\n|-|-|-|\n");

    for (index, subtask) in subtasks.iter().enumerate() {
        table += &format!(
            "| Subtask {} | {} | {} |\n",
            index + 1,
            subtask.constraints,
            subtask.max_score
        );
    }

    table
}
