use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Error};
use itertools::Itertools;

use task_maker_format::ioi::UIState;
use task_maker_format::ui::{StdoutPrinter, UIStateT, BLUE, YELLOW};
use task_maker_format::{cwrite, cwriteln, EvaluationConfig, SolutionCheckResult, TaskFormat};
use task_maker_lang::LanguageManager;

use crate::context::RuntimeContext;
use crate::tools::opt::AddSolutionChecksOpt;
use crate::LoggerOpt;

pub fn main_add_solution_checks(
    mut opt: AddSolutionChecksOpt,
    logger_opt: LoggerOpt,
) -> Result<(), Error> {
    opt.ui.disable_if_needed(&logger_opt);
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
    let mut changes_to_write = false;
    for solution_name in ui_state.solutions.keys() {
        let solution = &ui_state.solutions[solution_name];
        if solution.path.is_symlink() {
            continue;
        }
        if !solution.checks.is_empty() {
            skipped.push(&solution.name);
            continue;
        }
        let has_changes = process_solution(&ui_state, solution_name, opt.in_place);
        if has_changes && !opt.in_place {
            changes_to_write = true;
        }
        println!();
    }

    if !skipped.is_empty() {
        cwrite!(printer, YELLOW, "Warning");
        println!(
            ": These solutions already have at least one check, so they have been skipped: {}",
            skipped.iter().join(", ")
        );
    }
    if changes_to_write {
        cwrite!(printer, BLUE, "Note");
        println!(": The comments above have not been written to the solution files. To do this automatically pass -i.");
    }

    Ok(())
}

/// Generate (and add with in_place) the @check comments to this solution.
fn process_solution(state: &UIState, solution_name: &Path, in_place: bool) -> bool {
    let solution = &state.solutions[solution_name];
    let language = LanguageManager::detect_language(solution_name);

    let solution_results = if let Some(solution_results) = state.evaluations.get(solution_name) {
        solution_results
    } else {
        println!("Solution '{}' not evaluated, skipping", solution.name);
        return false;
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
            return false;
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
    if in_place && !comments.is_empty() {
        if let Err(e) = write_comments_to_file(&solution.path, &comments).with_context(|| {
            format!(
                "Failed to write @check comments to '{}'",
                solution.path.display()
            )
        }) {
            eprintln!("Error: {:?}", e);
        }
    }

    !comments.is_empty()
}

fn write_comments_to_file(path: &Path, comments: &[String]) -> Result<(), Error> {
    let mut file =
        File::open(path).with_context(|| format!("Failed to open '{}'", path.display()))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .context("Failed to read solution content")?;
    drop(file);

    // If the source file starts with the shebang, we cannot simply add the comments at the
    // beginning.
    let insert_position = if content.starts_with("#!") {
        content.find('\n').map(|n| n + 1).unwrap_or(0)
    } else {
        0
    };

    let comments = comments.iter().join("\n") + "\n";
    content.insert_str(insert_position, &comments);

    let mut file = File::options()
        .write(true)
        .open(path)
        .with_context(|| format!("Failed to open '{}' for writing", path.display()))?;
    file.write_all(content.as_bytes())
        .context("Failed to write the source file content")?;
    eprintln!("Written!");
    Ok(())
}
