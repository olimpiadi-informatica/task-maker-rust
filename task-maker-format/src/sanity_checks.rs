//! Sanity checks for IOI-like tasks.

use std::sync::Mutex;

use anyhow::Error;
use itertools::Itertools;
use task_maker_diagnostics::Diagnostic;

use crate::EvaluationData;

/// Trait that describes the behavior of a sanity check.
pub trait SanityCheck<Task>: Send + Sync + std::fmt::Debug {
    /// The name of the sanity check.
    fn name(&self) -> &'static str;

    /// This function will be called before the actual execution of the DAG. It can add new
    /// executions to the DAG.
    fn pre_hook(&mut self, _task: &Task, _eval: &mut EvaluationData) -> Result<(), Error> {
        Ok(())
    }

    /// This function will be called after the execution of the DAG completes.
    fn post_hook(&mut self, _task: &Task, _eval: &mut EvaluationData) -> Result<(), Error> {
        Ok(())
    }
}

/// Internal state of the sanity checks.
#[derive(Debug, Default)]
struct SanityChecksState<Task> {
    /// The list of enabled sanity checks.
    sanity_checks: Vec<Box<dyn SanityCheck<Task>>>,
}

/// Sanity checks for a IOI task.
#[derive(Debug)]
pub struct SanityChecks<Task> {
    /// The internal state of the sanity checks. Mutex to allow interior mutability and Send+Sync
    /// support.
    state: Mutex<SanityChecksState<Task>>,
}

impl<Task> SanityChecks<Task> {
    pub fn new(checks: Vec<Box<dyn SanityCheck<Task>>>) -> SanityChecks<Task> {
        SanityChecks {
            state: Mutex::new(SanityChecksState {
                sanity_checks: checks,
            }),
        }
    }

    /// Function called for the first pass of sanity checks of the task. This will check all the
    /// statically checkable properties of the task and may add some executions for checking dynamic
    /// properties of the task.
    ///
    /// This is executed after the DAG of the task is built.
    pub fn pre_hook(&self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        for check in state.sanity_checks.iter_mut() {
            if let Err(e) = check.pre_hook(task, eval) {
                eval.add_diagnostic(Diagnostic::warning(format!(
                    "Sanity check {} failed: {}",
                    check.name(),
                    e
                )))?;
            }
        }
        Ok(())
    }

    /// Function called after the evaluation completes. This will check that the produced assets are
    /// valid and the executions added by the pre_hook produced the correct results.
    pub fn post_hook(&self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        for check in state.sanity_checks.iter_mut() {
            if let Err(e) = check.post_hook(task, eval) {
                eval.add_diagnostic(Diagnostic::warning(format!(
                    "Sanity check {} failed: {}",
                    check.name(),
                    e
                )))?;
            }
        }
        Ok(())
    }
}

impl<Task> Default for SanityChecks<Task> {
    fn default() -> SanityChecks<Task> {
        SanityChecks {
            state: Mutex::new(SanityChecksState {
                sanity_checks: vec![],
            }),
        }
    }
}

/// Return a list of all the sanity check names.
pub fn get_sanity_check_list() -> Vec<String> {
    crate::ioi::sanity_checks::get_sanity_check_names()
        .iter()
        .chain(crate::terry::sanity_checks::get_sanity_check_names().iter())
        .map(ToString::to_string)
        .collect()
}

/// Return a comma separated list of the names of all the sanity checks.
pub fn get_sanity_check_names() -> String {
    get_sanity_check_list().into_iter().join(", ")
}
