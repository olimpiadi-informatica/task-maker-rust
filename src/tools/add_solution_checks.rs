use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Error};
use itertools::Itertools;

use task_maker_format::ioi::UIState;
use task_maker_format::ui::{StdoutPrinter, UIStateT, BLUE};
use task_maker_format::{cwriteln, EvaluationConfig, SolutionCheckResult, TaskFormat};
use task_maker_lang::LanguageManager;

use crate::context::RuntimeContext;
use crate::tools::opt::AddSolutionChecksOpt;

pub fn main_add_solution_checks(opt: AddSolutionChecksOpt) -> Result<(), Error> {
    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: false,
        no_statement: true,
        solution_paths: opt.solutions.iter().map(PathBuf::from).collect(),
        disabled_sanity_checks: Default::default(),
        seed: Default::default(),
        dry_run: true,
    };
    let task = opt
        .find_task
        .find_task(&eval_config)
        .context("Failed to locate the task")?;

    // This is a mutex because this state is updated in the UI thread, but it will later be used by
    // this main thread. In theory after executor.execute() the UI thread should have exited, so we
    // are the only owner of this state, but at the moment it's hard to express.
    let ui_state = Arc::new(Mutex::new(None::<UIState>));

    // setup the configuration and the evaluation metadata
    let context = RuntimeContext::new(task, &opt.execution, |task, eval| {
        // build the DAG for the task
        task.build_dag(eval, &eval_config)
            .context("Cannot build the task DAG")?;
        let ioi_task = match &task {
            TaskFormat::IOI(task) => {
                if task.subtasks.values().any(|st| st.name.is_none()) {
                    bail!("Not all the subtasks have a name, cannot proceed");
                }
                task
            }
            _ => bail!("The add-solution-checks tool only supports IOI-tasks for now"),
        };
        *ui_state.lock().unwrap() = Some(UIState::new(ioi_task, eval.dag.data.config.clone()));
        Ok(())
    })?;

    // start the execution
    let executor = context.connect_executor(&opt.execution, &opt.storage)?;
    let executor = executor.start_ui(&opt.ui.ui, {
        let ui_state = ui_state.clone();
        move |ui, message| {
            ui.on_message(message.clone());
            ui_state.lock().unwrap().as_mut().unwrap().apply(message);
        }
    })?;
    executor.execute()?;

    let mut printer = StdoutPrinter::default();
    cwriteln!(printer, BLUE, "Solution checks");
    let ui_state = ui_state.lock().unwrap().take().unwrap();
    let mut skipped = vec![];
    for solution_name in ui_state.solutions.keys() {
        let solution = &ui_state.solutions[solution_name];
        if solution.path.is_symlink() {
            continue;
        }
        if !solution.checks.is_empty() {
            skipped.push(&solution.name);
            continue;
        }
        process_solution(&ui_state, solution_name);
        println!();
    }

    if !skipped.is_empty() {
        println!(
            "These solutions already have at least one check, so they have been skipped: {}",
            skipped.iter().join(", ")
        );
    }

    Ok(())
}

fn process_solution(state: &UIState, solution_name: &Path) {
    let solution = &state.solutions[solution_name];
    let language = LanguageManager::detect_language(solution_name);

    let solution_results = if let Some(solution_results) = state.evaluations.get(solution_name) {
        solution_results
    } else {
        println!("Solution '{}' not evaluated, skipping", solution.name);
        return;
    };

    let mut checks: HashMap<_, Vec<_>> = HashMap::new();
    for st_num in solution_results.subtasks.keys().sorted() {
        let st_info = &solution_results.subtasks[st_num];
        let subtask = &state.task.subtasks[st_num];

        let testcase_results: Vec<Option<SolutionCheckResult>> = st_info
            .testcases
            .values()
            .map(|testcase| (&testcase.status).into())
            .collect_vec();
        // Not all the testcase results are valid.
        if testcase_results.iter().any(Option::is_none) {
            println!(
                "Solution '{}' not evaluated on all the testcases, skipping.",
                solution.name
            );
            return;
        }
        let testcase_results: HashSet<_> =
            testcase_results.into_iter().map(Option::unwrap).collect();

        // "Accepted" must be present only if all it's Accepted.
        if testcase_results.len() == 1 && testcase_results.contains(&SolutionCheckResult::Accepted)
        {
            checks
                .entry(SolutionCheckResult::Accepted)
                .or_default()
                .push(subtask.name.as_ref().unwrap());
        } else {
            // At least one is not Accepted...
            for result in testcase_results {
                // ...but Accepted may still be present in this list.
                if result == SolutionCheckResult::Accepted {
                    continue;
                }
                checks
                    .entry(result)
                    .or_default()
                    .push(subtask.name.as_ref().unwrap());
            }
        }
    }

    let comments = checks
        .into_iter()
        .sorted_by_key(|(result, _)| *result)
        .map(|(result, subtasks)| {
            let prefix = language
                .as_ref()
                .and_then(|lang| lang.inline_comment_prefix())
                .unwrap_or_default();
            let subtasks = subtasks.iter().join(" ");
            format!("{} @check-{}: {}", prefix, result.as_str(), subtasks)
        })
        .collect_vec();
    println!("{}\n{}", solution.name, comments.iter().join("\n"));
}
