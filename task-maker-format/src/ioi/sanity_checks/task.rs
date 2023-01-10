use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs,
};

use anyhow::Error;
use regex::Regex;
use task_maker_diagnostics::{CodeSpan, Diagnostic};

use crate::ioi::IOITask;
use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::{list_files, EvaluationData, SolutionCheckResult};

/// The default maximum score of a task.
const DEFAULT_TASK_MAX_SCORE: f64 = 100.0;

/// Check that the task has the usual maximum score.
#[derive(Debug, Default)]
pub struct TaskMaxScore;
make_sanity_check!(TaskMaxScore);

impl SanityCheck for TaskMaxScore {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "TaskMaxScore"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Task
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let task_score: f64 = task.subtasks.values().map(|st| st.max_score).sum();
        if approx::abs_diff_ne!(task_score, DEFAULT_TASK_MAX_SCORE) {
            eval.add_diagnostic(Diagnostic::error(format!(
                "The score of the task is {} (not {})",
                task_score, DEFAULT_TASK_MAX_SCORE
            )))?;
        }
        Ok(())
    }
}

/// Check that there are no broken links.
#[derive(Debug, Default)]
pub struct BrokenSymlinks;
make_sanity_check!(BrokenSymlinks);

impl SanityCheck for BrokenSymlinks {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "BrokenSymlinks"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Task
    }

    fn post_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        for file in list_files(&task.path, vec!["**/*"]) {
            if !file.exists() {
                let path = task.path_of(&file);
                // Ignore the symlinks here as they are not interesting.
                if path.starts_with("fuzz") {
                    continue;
                }
                if let Ok(content) = file.read_link() {
                    eval.add_diagnostic(
                        Diagnostic::warning(format!("{} is a broken symlink", path.display()))
                            .with_note(format!("It points to {}", content.display())),
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Check that cpp source files don't contain #include <bits/stdc++.h> (or whitespace variants of it)
#[derive(Debug, Default)]
pub struct NoBitsStdCpp;
make_sanity_check!(NoBitsStdCpp);

impl SanityCheck for NoBitsStdCpp {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "NoBitsStdCpp"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Task
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        lazy_static! {
            static ref RE: Regex =
                Regex::new(r###"(?m)^#\s*include\s*(?:<|")bits/stdc\+\+\.h(?:>|").*$"###)
                    .expect("Invalid regex");
        }

        for att in list_files(&task.path, vec!["**/*.cpp", "**/*.cc"]) {
            let path = task.path_of(&att);
            if let Ok(content) = fs::read_to_string(path) {
                if let Some(span) = RE.find(&content) {
                    eval.add_diagnostic(
                        Diagnostic::warning(format!(
                            "bits/stdc++.h included from {}",
                            path.display()
                        ))
                        .with_note("This won't compile under Clang")
                        .with_code_span(CodeSpan::from_str(
                            path,
                            &content,
                            span.start(),
                            span.end() - span.start(),
                        )?),
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Check that "implied" subtasks dependecies do not form cycles.
#[derive(Debug, Default)]
pub struct SubtaskDependencies;
make_sanity_check!(SubtaskDependencies);

impl SanityCheck for SubtaskDependencies {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "SubtaskDependencies"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Task
    }

    fn post_hook(&self, task: &Self::Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut table = HashSet::new();
        for &st1 in task.subtasks.keys() {
            for &st2 in task.subtasks.keys() {
                if st1 != st2 {
                    table.insert((st1, st2));
                }
            }
        }

        for sol in &eval.solutions {
            let mut map = HashMap::new();
            for check in &sol.checks {
                let val = match check.result {
                    SolutionCheckResult::Accepted => 1.0,
                    SolutionCheckResult::PartialScore => f32::NAN,
                    _ => 0.0,
                };

                for subtask in task.find_subtasks_by_pattern_name(&check.subtask_name_pattern) {
                    map.insert(subtask.id, val);
                }
            }

            for (&k1, v1) in &map {
                for (&k2, v2) in &map {
                    if matches!(v1.partial_cmp(v2), None | Some(Ordering::Greater)) {
                        table.remove(&(k1, k2));
                    }
                }
            }
        }

        'outer: for &st1 in task.subtasks.keys() {
            let mut sts = vec![st1];
            for &st2 in task.subtasks.keys() {
                if table.contains(&(st1, st2)) && table.contains(&(st2, st1)) {
                    if st1 < st2 {
                        sts.push(st2);
                    } else {
                        continue 'outer;
                    }
                }
            }
            if sts.len() > 1 {
                sts.sort();
                eval.add_diagnostic(
                    Diagnostic::warning(format!(
                        "Subtasks {sts:?} are solved by the same set of solutions",
                    ))
                    .with_note("Add a solution that solves only one of them"),
                )?;
            }
        }

        let mut to_swap = Vec::new();
        for &st1 in task.subtasks.keys() {
            for &st2 in task.subtasks.keys() {
                if st1 < st2 && table.contains(&(st1, st2)) && !table.contains(&(st2, st1)) {
                    to_swap.push((st1, st2));
                }
            }
        }

        if !to_swap.is_empty() {
            eval.add_diagnostic(
                Diagnostic::warning("Subtasks are not in order of difficulty").with_note(format!(
                    "Based on the current solutions the following pairs of subtasks seems to be ordered incorrectly {to_swap:?}"
                )),
            )?;
        }

        Ok(())
    }
}
