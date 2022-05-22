use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use task_maker_exec::ExecutorStatus;
use task_maker_format::ioi::TestcaseId;
use task_maker_format::ui::{UIExecutionStatus, UIMessage, UIStateT};

use crate::tools::find_bad_case::dag::{Batch, TestcaseData};
use crate::tools::find_bad_case::FindBadCaseOpt;

#[derive(Debug, Clone)]
pub struct UIState {
    pub stop_evaluation: StopEvaluation,

    pub solution: PathBuf,
    pub generator_args: Vec<String>,
    pub batch_size: usize,

    pub executor_status: Option<ExecutorStatus<SystemTime>>,
    pub progress: Progress,
    pub batches: Vec<CurrentBatch>,

    pub shared: Arc<RwLock<SharedUIState>>,
}

#[derive(Clone)]
pub struct StopEvaluation(Arc<dyn Fn() + Send + Sync>);

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub inputs_generated: usize,
    pub inputs_solved: usize,
    pub generator_time_sum: f64,
    pub solution_time_sum: f64,
}

#[derive(Debug, Clone)]
pub struct CurrentBatch {
    pub testcase_status: Vec<TestcaseStatus>,
}

impl CurrentBatch {
    fn new(batch_size: usize) -> Self {
        Self {
            testcase_status: (0..batch_size).map(|_| TestcaseStatus::Pending).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TestcaseStatus {
    Pending,
    Generating,
    Generated,
    Validating,
    Validated,
    Solving,
    Solved,
    Checking,
    Success,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct SharedUIState {
    pub batch_index: usize,
    pub should_stop: bool,
    pub current_batch: Option<Batch>,
    pub failing_testcase: Option<(TestcaseData, String)>,
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
                    set(testcase, TestcaseStatus::Generated);
                    self.progress.inputs_generated += 1;
                    self.progress.generator_time_sum += result.resources.cpu_time;
                }
                _ => {}
            },
            UIMessage::IOIValidation {
                testcase, status, ..
            } => match status {
                UIExecutionStatus::Started { .. } => set(testcase, TestcaseStatus::Validating),
                UIExecutionStatus::Done { .. } => set(testcase, TestcaseStatus::Validated),
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
                testcase: testcase_id,
                score,
                message,
                ..
            } => {
                let testcase = &mut self.batches.last_mut().unwrap().testcase_status
                    [testcase_id as usize % self.batch_size];
                if score == 1.0 {
                    *testcase = TestcaseStatus::Success;
                } else {
                    *testcase = TestcaseStatus::Failed(message.clone());
                    let mut shared = self.shared.write().unwrap();
                    let testcase = shared
                        .current_batch
                        .as_ref()
                        .unwrap()
                        .testcases
                        .get(&testcase_id);
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
