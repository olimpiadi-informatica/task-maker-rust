//! Sanity checks for IOI-like tasks.

use std::sync::Mutex;

use anyhow::Error;
use task_maker_diagnostics::Diagnostic;

use crate::EvaluationData;

/// Category of a sanity check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SanityCheckCategory {
    /// The sanity check verifies the attachments.
    Attachments,
    /// The sanity check verifies the checker.
    Checker,
    /// The sanity check verifies the input/output files.
    Io,
    /// The sanity check verifies the solutions.
    Solutions,
    /// The sanity check verifies the statement files.
    Statement,
    /// The sanity check verifies general properties of the task.
    Task,
}

impl SanityCheckCategory {
    /// What this category is about.
    pub fn purpose(&self) -> &'static str {
        match self {
            SanityCheckCategory::Attachments => "verifies the attachments",
            SanityCheckCategory::Checker => "verifies the checker",
            SanityCheckCategory::Io => "verifies the input/output files",
            SanityCheckCategory::Solutions => "verifies the solutions",
            SanityCheckCategory::Statement => "verifies the statement files",
            SanityCheckCategory::Task => "verifies general properties of the task",
        }
    }

    /// String version of this category.
    pub fn as_str(&self) -> &'static str {
        match self {
            SanityCheckCategory::Attachments => "Attachments",
            SanityCheckCategory::Checker => "Checker",
            SanityCheckCategory::Io => "Io",
            SanityCheckCategory::Solutions => "Solutions",
            SanityCheckCategory::Statement => "Statement",
            SanityCheckCategory::Task => "Task",
        }
    }
}

/// Trait that describes the behavior of a sanity check.
pub trait SanityCheck<Task>: Send + Sync + std::fmt::Debug {
    /// The name of the sanity check.
    fn name(&self) -> &'static str;

    /// The category of the sanity check.
    fn category(&self) -> SanityCheckCategory;

    /// This function will be called before the actual execution of the DAG. It can add new
    /// executions to the DAG.
    fn pre_hook(&self, _task: &Task, _eval: &mut EvaluationData) -> Result<(), Error> {
        Ok(())
    }

    /// This function will be called after the execution of the DAG completes.
    fn post_hook(&self, _task: &Task, _eval: &mut EvaluationData) -> Result<(), Error> {
        Ok(())
    }
}

/// Register this struct as a sanity check.
///
/// ## Usage
///
/// ```ignore
/// struct SanityCheckName;
/// make_sanity_check!(SanityCheckName);
/// ```
macro_rules! make_sanity_check {
    ($name:tt) => {
        paste::paste! {
            #[allow(non_upper_case_globals)]
            static [<__ $name _SANITY_CHECK>]: $name = $name;
            ::inventory::submit!(&[<__ $name _SANITY_CHECK>] as &dyn SanityCheck<_>);
        }
    };
}
pub(crate) use make_sanity_check;

/// Internal state of the sanity checks.
#[derive(Debug, Default)]
struct SanityChecksState<Task: 'static> {
    /// The list of enabled sanity checks.
    sanity_checks: Vec<&'static dyn SanityCheck<Task>>,
}

/// Sanity checks for a IOI task.
#[derive(Debug)]
pub struct SanityChecks<Task: 'static> {
    /// The internal state of the sanity checks. Mutex to allow interior mutability and Send+Sync
    /// support.
    state: Mutex<SanityChecksState<Task>>,
}

impl<Task> SanityChecks<Task> {
    pub fn new(checks: Vec<&'static dyn SanityCheck<Task>>) -> SanityChecks<Task> {
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

/// Return a list of all the sanity check.
pub fn get_sanity_check_list() -> Vec<(&'static str, SanityCheckCategory)> {
    let ioi = crate::ioi::sanity_checks::get_sanity_check_list(&[])
        .into_iter()
        .map(|check| (check.name(), check.category()));
    let terry = crate::terry::sanity_checks::get_sanity_check_list(&[])
        .into_iter()
        .map(|check| (check.name(), check.category()));
    ioi.chain(terry).collect()
}
