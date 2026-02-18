use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use task_maker_dag::ExecutionResult;
use task_maker_exec::ExecutorStatus;
use task_maker_format::ioi::TestcaseId;
use task_maker_format::ui::{UIExecutionStatus, UIMessage, UIStateT};

use crate::tools::find_bad_case::dag::{Batch, TestcaseData};
use crate::tools::find_bad_case::FindBadCaseOpt;

/// This state is owned by the UI.
///
/// It contains a shared state that can be accessed by the outside world.
#[derive(Debug, Clone)]
pub struct UIState {
    /// A callback that can be used by the UI to stop the current executor.
    pub stop_evaluation: StopEvaluation,

    /// The path to the solution to evaluate.
    pub solution: PathBuf,
    /// The template arguments passed to the generator.
    pub generator_args: Vec<String>,
    /// The size of the batch.
    pub batch_size: usize,

    /// The current status of the executor, if any.
    pub executor_status: Option<ExecutorStatus<SystemTime>>,
    /// The current progress of the evaluation.
    pub progress: Progress,
    /// The set of batches that have been run.
    pub batches: Vec<CurrentBatch>,

    /// The part of the state shared with the outside world (i.e. by non-UI code).
    pub shared: Arc<RwLock<SharedUIState>>,
}

/// A wrapper to the callback that can be used by the UI to stop the current executor.
#[derive(Clone)]
pub struct StopEvaluation(Arc<dyn Fn() + Send + Sync>);

/// The current progress of the evaluation.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// The number of input files that have been generated.
    pub inputs_generated: usize,
    /// The number of input cases that have been solved.
    pub inputs_solved: usize,
    /// The sum of the times of the generators. This can be used to compute the average execution
    /// time.
    pub generator_time_sum: f64,
    /// The sum of the times of the solution. This can be used to compute the average execution
    /// time.
    pub solution_time_sum: f64,
}

/// Information about the status of the current batch.
#[derive(Debug, Clone)]
pub struct CurrentBatch {
    /// The information about each testcase in the batch.
    pub testcase_status: Vec<TestcaseStatus>,
}

impl CurrentBatch {
    fn new(batch_size: usize) -> Self {
        Self {
            testcase_status: (0..batch_size).map(|_| TestcaseStatus::Pending).collect(),
        }
    }
}

/// The status of the evaluation of each testcase.
#[derive(Debug, Clone)]
pub enum TestcaseStatus {
    /// The testcase has not been generated yey.
    Pending,
    /// The generator is running.
    Generating,
    /// The generator has run, waiting for the validator.
    Generated,
    /// The validator is running.
    Validating,
    /// The validator has run, waiting for the solution.
    Validated,
    /// The solution is running.
    Solving,
    /// The solution has run, waiting for the checker.
    Solved,
    /// The checker is running.
    Checking,
    /// The checker has run, and the solution has solved the testcase correctly.
    Success,
    /// The solution failed to solve the testcase.
    Failed(String),
    /// An error occurred while producing the testcase.
    Error,
}

/// This is the state shared between the UI and the non-UI code.
#[derive(Debug, Clone, Default)]
pub struct SharedUIState {
    /// The index of the current batch.
    pub batch_index: usize,
    /// Whether the UI and the execution should stop and no further batch should be tried.
    pub should_stop: bool,
    /// The last batch being evaluated.
    pub last_batch: Option<Batch>,
    /// A testcase that made the solution fail, together with a failing message.
    pub failing_testcase: Option<(TestcaseData, String)>,
    /// A testcase that failed to generate, together with a message and the result of the execution.
    pub errored_testcase: Option<(TestcaseData, String, ExecutionResult)>,
}

impl UIState {
    pub fn new(opt: &FindBadCaseOpt, stop_evaluation: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            stop_evaluation: StopEvaluation::new(stop_evaluation),
            solution: opt.solution.clone(),
            generator_args: opt.generator_args.clone(),
            batch_size: opt.batch_size,
            executor_status: None,
            batches: vec![],
            progress: Default::default(),
            shared: Arc::new(RwLock::new(SharedUIState::default())),
        }
    }
}

impl UIStateT for UIState {
    fn apply(&mut self, message: UIMessage) {
        let mut set = |testcase: TestcaseId, state: TestcaseStatus| {
            let testcase = &mut self.batches.last_mut().unwrap().testcase_status
                [testcase as usize % self.batch_size];
            *testcase = state;
        };
        match message {
            UIMessage::IOITask { .. } => {
                self.batches.push(CurrentBatch::new(self.batch_size));
            }
            UIMessage::ServerStatus { status } => self.executor_status = Some(status),
            UIMessage::IOIGeneration {
                testcase, status, ..
            } => match status {
                UIExecutionStatus::Started { .. } => set(testcase, TestcaseStatus::Generating),
                UIExecutionStatus::Done { result } => {
                    self.progress.inputs_generated += 1;
                    self.progress.generator_time_sum += result.resources.cpu_time;
                    if result.status.is_success() {
                        set(testcase, TestcaseStatus::Generated);
                    } else {
                        set(testcase, TestcaseStatus::Error);
                        let mut shared = self.shared.write().unwrap();
                        let testcase = shared
                            .last_batch
                            .as_ref()
                            .unwrap()
                            .testcases
                            .get(&testcase)
                            .unwrap();
                        shared.errored_testcase =
                            Some((testcase.clone(), "Generator failed".into(), result));
                    }
                }
                _ => {}
            },
            UIMessage::IOIValidation {
                testcase, status, ..
            } => match status {
                UIExecutionStatus::Started { .. } => set(testcase, TestcaseStatus::Validating),
                UIExecutionStatus::Done { result } => {
                    if result.status.is_success() {
                        set(testcase, TestcaseStatus::Validated);
                    } else {
                        set(testcase, TestcaseStatus::Error);
                        let mut shared = self.shared.write().unwrap();
                        let testcase = shared
                            .last_batch
                            .as_ref()
                            .unwrap()
                            .testcases
                            .get(&testcase)
                            .unwrap();
                        shared.errored_testcase =
                            Some((testcase.clone(), "Validator failed".into(), result));
                    }
                }
                _ => {}
            },
            UIMessage::IOIEvaluation {
                testcase, status, ..
            } => match status {
                UIExecutionStatus::Started { .. } => set(testcase, TestcaseStatus::Solving),
                UIExecutionStatus::Done { result } => {
                    if result.status.is_success() {
                        set(testcase, TestcaseStatus::Solved);
                    }
                    self.progress.inputs_solved += 1;
                    self.progress.solution_time_sum += result.resources.cpu_time;
                }
                _ => {}
            },
            UIMessage::IOIChecker {
                testcase,
                status: UIExecutionStatus::Started { .. },
                ..
            } => {
                set(testcase, TestcaseStatus::Checking);
            }
            UIMessage::IOITestcaseScore {
                testcase,
                score,
                message,
                ..
            } => {
                if score == 1.0 {
                    set(testcase, TestcaseStatus::Success);
                } else {
                    set(testcase, TestcaseStatus::Failed(message.clone()));
                    let mut shared = self.shared.write().unwrap();
                    let testcase = shared.last_batch.as_ref().unwrap().testcases.get(&testcase);
                    shared.failing_testcase = testcase.map(|tc| (tc.clone(), message));
                    shared.should_stop = true;
                    self.stop_evaluation.stop();
                }
            }
            _ => {}
        }
    }

    fn finish(&mut self) {}
}

impl StopEvaluation {
    fn new(stop_evaluation: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(stop_evaluation))
    }

    fn stop(&self) {
        (self.0)()
    }
}

impl Debug for StopEvaluation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StopEvaluation").finish()
    }
}
