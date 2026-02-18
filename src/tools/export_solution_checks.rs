use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Error};
use clap::Parser;
use serde::{Deserialize, Serialize};
use task_maker_format::ioi::IOITask;
use task_maker_format::{
    EvaluationConfig, EvaluationData, Solution, SolutionCheckResult, TaskFormat,
};

use crate::{FilterOpt, FindTaskOpt};

#[derive(Parser, Debug, Clone)]
pub struct ExportSolutionChecksOpt {
    #[clap(flatten, next_help_heading = Some("TASK SEARCH"))]
    pub find_task: FindTaskOpt,

    #[clap(flatten, next_help_heading = Some("FILTER"))]
    pub filter: FilterOpt,
}

#[derive(Serialize, Deserialize)]
struct SolutionWithChecks {
    path: PathBuf,
    checks: Vec<Option<SolutionCheckResult>>,
    min_score: f64,
    max_score: f64,
}

pub fn main_export_solution_checks(opt: ExportSolutionChecksOpt) -> Result<(), Error> {
    let eval_config = EvaluationConfig {
        solution_filter: opt.filter.filter,
        booklet_solutions: false,
        no_statement: true,
        solution_paths: opt.filter.solution,
        disabled_sanity_checks: Default::default(),
        seed: Default::default(),
        dry_run: true,
    };
    let task = opt
        .find_task
        .find_task(&eval_config)
        .context("Failed to locate the task")?;

    let TaskFormat::IOI(task) = task else {
        bail!("Exporting solution checks is only supported for IOI tasks")
    };

    let (mut eval, _) = EvaluationData::new(task.path());
    let solutions = eval_config.find_solutions(
        task.path(),
        vec!["sol/*"],
        Some(task.grader_map.clone()),
        &mut eval,
    );

    let subtasks = task
        .subtasks
        .iter()
        .filter_map(|(_, info)| info.name.clone().map(|name| (name, info.id)))
        .collect::<HashMap<_, _>>();

    let checks = solutions
        .iter()
        .map(|solution| extract_solution_checks(&task, &subtasks, solution))
        .collect::<Result<Vec<_>, _>>()?;

    println!("{}", serde_json::to_string_pretty(&checks)?);

    Ok(())
}

fn extract_solution_checks(
    task: &IOITask,
    subtasks: &HashMap<String, u32>,
    solution: &Solution,
) -> anyhow::Result<SolutionWithChecks> {
    let mut checks = vec![None; task.subtasks.len()];
    for check in &solution.checks {
        if let Some(&idx) = subtasks.get(&check.subtask_name_pattern) {
            let idx: usize = idx.try_into()?;
            if checks[idx].is_some() {
                bail!(
                    "Found multiple checks for subtask {} in solution {}",
                    check.subtask_name_pattern,
                    solution.source_file.path.display()
                );
            }
            checks[idx] = Some(check.result);
        } else if check.subtask_name_pattern == "*" {
            for subtask_check in checks.iter_mut() {
                if subtask_check.is_some() {
                    bail!(
                        "Found multiple checks for subtask {} in solution {}",
                        check.subtask_name_pattern,
                        solution.source_file.path.display()
                    );
                }
                *subtask_check = Some(check.result);
            }
        } else {
            bail!(
                "Found invalid subtask check {} in solution {}",
                check.subtask_name_pattern,
                solution.source_file.path.display()
            );
        }
    }

    let mut min_score = 0.;
    let mut max_score = 0.;

    for (i, check) in checks.iter().enumerate() {
        if *check == Some(SolutionCheckResult::Accepted) {
            min_score += task.subtasks[&i.try_into()?].max_score;
            max_score += task.subtasks[&i.try_into()?].max_score;
        } else if *check == Some(SolutionCheckResult::PartialScore) || check.is_none() {
            max_score += task.subtasks[&i.try_into()?].max_score;
        }
    }

    Ok(SolutionWithChecks {
        path: solution.source_file.path.clone(),
        checks,
        min_score,
        max_score,
    })
}
