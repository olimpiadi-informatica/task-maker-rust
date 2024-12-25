use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Error};
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::LoggerOpt;
use task_maker_format::terry::TerryTask;
use task_maker_format::{find_task, EvaluationConfig};

#[derive(Parser, Debug, Clone)]
pub struct TerryStatementOpt {
    /// Path to statement template (uses task directory structure if omitted)
    #[clap(long = "statement-path", short = 's')]
    pub statement_path: Option<String>,

    /// Path to subtasks file (none if omitted)
    #[clap(long = "subtasks-path", short = 't')]
    pub subtasks_path: Option<String>,

    /// Path to store output statement (stdout if omitted)
    #[clap(long = "output-path", short = 'o')]
    pub output_path: Option<String>,

    /// Look at most for this number of parents for searching the task
    #[clap(long = "max-depth", default_value = "3")]
    pub max_depth: u32,
}

/// A subtask has a maximum score and a list of testcases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementSubtask {
    /// The maximum score on the subtask
    pub max_score: f64,
    /// The additional constraints, as seen by the contestant
    pub constraints: String,
    /// The testcases in the subtask
    pub testcases: Vec<usize>,
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

    let (statement_path, subtasks_path, output_path) =
        if let Some(statement_path) = opt.statement_path {
            (
                PathBuf::from(statement_path),
                opt.subtasks_path.map(PathBuf::from),
                opt.output_path.map(PathBuf::from),
            )
        } else {
            let task = find_task(None, opt.max_depth, &eval_config)?;
            let path = task.path();
            let task = TerryTask::new(path, &eval_config)
                .with_context(|| format!("There is no Terry task at {}", path.display()))?;

            let Some(statement) = task.statement else {
                return Ok(());
            };

            (
                statement.path,
                statement.subtasks,
                Some(path.join("statement/statement.md")),
            )
        };

    let content = fs::read_to_string(statement_path)?;

    let new_content = if content.contains("<subtasks-recap/>") {
        let subtasks_path = subtasks_path.ok_or(anyhow!("No subtasks.yaml file."))?;
        let subtasks_content = fs::read_to_string(subtasks_path)?;
        let subtasks: Vec<_> = serde_yaml::from_str(&subtasks_content)?;
        let subtasks = generate_md_table(&subtasks);

        content.replace("<subtasks-recap/>", &subtasks)
    } else {
        content
    };

    match output_path {
        Some(output_file) => fs::write(output_file, new_content)?,
        None => print!("{}", new_content),
    }

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
