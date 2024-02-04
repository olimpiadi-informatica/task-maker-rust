use std::{
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
                Regex::new(r#"(?m)^#\s*include\s*(?:<|")bits/stdc\+\+\.h(?:>|").*$"#)
                    .expect("Invalid regex");
        }

        let mut diagnostic = None;

        for att in list_files(&task.path, vec!["**/*.cpp", "**/*.cc"]) {
            let path = task.path_of(&att);
            if let Ok(content) = fs::read_to_string(path) {
                if let Some(span) = RE.find(&content) {
                    diagnostic = Some(
                        diagnostic
                            .unwrap_or_else(|| {
                                Diagnostic::warning(r#"Usage of "bits/stdc++.h" is discouraged"#)
                                    .with_note("This won't compile under Clang")
                            })
                            .with_code_span(CodeSpan::from_str(
                                path,
                                &content,
                                span.start(),
                                span.end() - span.start(),
                            )?),
                    );
                }
            }
        }

        if let Some(diagnostic) = diagnostic {
            eval.add_diagnostic(diagnostic)?;
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

    fn pre_hook(&self, task: &Self::Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let non_zero_sts = task
            .subtasks
            .keys()
            .copied()
            .filter(|st| task.subtasks[st].max_score > 0.0)
            .collect::<Vec<_>>();

        let mut st_dependencies = HashSet::new();
        for &st1 in &non_zero_sts {
            for &st2 in &non_zero_sts {
                if st1 != st2 {
                    st_dependencies.insert((st1, st2));
                }
            }
        }

        let mut any_st_check = false;
        for sol in &eval.solutions {
            let mut score_range = HashMap::new();
            for check in &sol.checks {
                any_st_check = true;

                let val = match check.result {
                    SolutionCheckResult::Accepted => (1.0, 1.0),
                    SolutionCheckResult::PartialScore => (0.0, 1.0),
                    _ => (0.0, 0.0),
                };

                for subtask in task.find_subtasks_by_pattern_name(&check.subtask_name_pattern) {
                    score_range.insert(subtask.id, val);
                }
            }

            for &st1 in &non_zero_sts {
                for &st2 in &non_zero_sts {
                    if let (Some(v1), Some(v2)) = (score_range.get(&st1), score_range.get(&st2)) {
                        if v1.1 > v2.0 {
                            st_dependencies.remove(&(st1, st2));
                        }
                    }
                }
            }
        }

        if !any_st_check {
            return Ok(());
        }

        'outer: for &st1 in &non_zero_sts {
            let mut sts = vec![st1];
            for &st2 in &non_zero_sts {
                if st_dependencies.contains(&(st1, st2)) && st_dependencies.contains(&(st2, st1)) {
                    if st1 < st2 {
                        sts.push(st2);
                    } else {
                        continue 'outer;
                    }
                }
            }
            if sts.len() > 1 {
                sts.sort();
                let st_names: Vec<_> = sts
                    .iter()
                    .map(|st| {
                        task.subtasks[st]
                            .name
                            .as_ref()
                            .map(|s| s as &dyn std::fmt::Debug)
                            .unwrap_or(st)
                    })
                    .collect();
                eval.add_diagnostic(
                    Diagnostic::warning(format!(
                        "Subtasks {st_names:?} are solved by the same set of solutions",
                    ))
                    .with_note("Add a solution that solves only one of them"),
                )?;
            }
        }

        let mut to_swap = Vec::new();
        for &st1 in &non_zero_sts {
            for &st2 in &non_zero_sts {
                if st1 < st2
                    && st_dependencies.contains(&(st1, st2))
                    && !st_dependencies.contains(&(st2, st1))
                {
                    to_swap.push((st1, st2));
                }
            }
        }

        if !to_swap.is_empty() {
            let to_swap_names = to_swap
                .iter()
                .map(|(st1, st2)| {
                    let st1_name = task.subtasks[st1]
                        .name
                        .as_ref()
                        .map(|s| s as &dyn std::fmt::Debug)
                        .unwrap_or(st1);
                    let st2_name = task.subtasks[st2]
                        .name
                        .as_ref()
                        .map(|s| s as &dyn std::fmt::Debug)
                        .unwrap_or(st2);
                    (st1_name, st2_name)
                })
                .collect::<Vec<_>>();
            eval.add_diagnostic(
                Diagnostic::warning("Subtasks are not in order of difficulty").with_note(format!(
                    "Based on the current solutions the following pairs of subtasks seems to be ordered incorrectly {to_swap_names:?}"
                )),
            )?;
        }

        Ok(())
    }
}

/// Check that the task title is not emtpy.
#[derive(Debug, Default)]
pub struct EmptyTitle;
make_sanity_check!(EmptyTitle);

impl SanityCheck for EmptyTitle {
    type Task = IOITask;

    fn name(&self) -> &'static str {
        "EmptyTitle"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Task
    }

    fn pre_hook(&self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        if task.title.is_empty() {
            eval.add_diagnostic(Diagnostic::error("Missing task's title"))?;
        }
        Ok(())
    }
}
