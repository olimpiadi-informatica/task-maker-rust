use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use task_maker_dag::{ExecutionResult, ExecutionStatus};
use task_maker_exec::ExecutorStatus;

use crate::solution::SolutionInfo;
use crate::terry::finish_ui;
use crate::terry::{Seed, SolutionOutcome, TerryTask};
use crate::ui::{CompilationStatus, FinishUI, UIExecutionStatus, UIMessage, UIStateT};

/// The state of a Terry task, all the information for the UI are stored here.
#[derive(Debug, Clone)]
pub struct UIState {
    /// The task.
    pub task: TerryTask,
    /// The status of the compilations.
    pub compilations: HashMap<PathBuf, CompilationStatus>,
    /// The state of the solutions known.
    pub solutions: HashMap<PathBuf, SolutionState>,
    /// The status of the executor.
    pub executor_status: Option<ExecutorStatus<SystemTime>>,
    /// All the emitted warnings.
    pub warnings: Vec<String>,
    /// All the emitted errors.
    pub errors: Vec<String>,
}

/// The state of the evaluation of a solution.
#[derive(Debug, Clone)]
pub struct SolutionState {
    /// The information about this solution.
    pub info: SolutionInfo,
    /// The status of the evaluation.
    pub status: SolutionStatus,
    /// The checker's outcome or its error.
    pub outcome: Option<Result<SolutionOutcome, String>>,
    /// The seed used for the generation.
    pub seed: Option<Seed>,
    /// The result of the generator.
    pub generator_result: Option<ExecutionResult>,
    /// The result of the validator.
    pub validator_result: Option<ExecutionResult>,
    /// The result of the solution.
    pub solution_result: Option<ExecutionResult>,
    /// The result of the checker.
    pub checker_result: Option<ExecutionResult>,
}

/// The status of the evaluation of a solution.
#[derive(Debug, Clone)]
pub enum SolutionStatus {
    /// The solution has not started yet.
    Pending,
    /// The generator is preparing the input file.
    Generating,
    /// The generator prepared the input file.
    Generated,
    /// The validator is running.
    Validating,
    /// The validator checked the input file.
    Validated,
    /// The solution is running.
    Solving,
    /// The solution produced the output file.
    Solved,
    /// The checker is running.
    Checking,
    /// The checker completed successfully.
    Done,
    /// Something failed during the evaluation.
    Failed(String),
    /// The evaluation has been skipped.
    Skipped,
}

impl Default for SolutionStatus {
    fn default() -> Self {
        SolutionStatus::Pending
    }
}

impl SolutionState {
    fn new(info: SolutionInfo) -> Self {
        Self {
            info,
            status: Default::default(),
            outcome: Default::default(),
            seed: Default::default(),
            generator_result: Default::default(),
            validator_result: Default::default(),
            solution_result: Default::default(),
            checker_result: Default::default(),
        }
    }
}

impl UIState {
    /// Make a new `UIState`.
    pub fn new(task: &TerryTask) -> UIState {
        UIState {
            task: task.clone(),
            compilations: HashMap::new(),
            solutions: HashMap::new(),
            executor_status: None,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl UIStateT for UIState {
    /// Apply a `UIMessage` to this state.
    fn apply(&mut self, message: UIMessage) {
        macro_rules! process_step {
            ($self:expr, $solution:expr, $status:expr, $step_result:tt, $start_status:tt, $ok_status:tt, $name:literal) => {{
                let sol = $self
                    .solutions
                    .get_mut(&$solution)
                    .expect("Outcome of an unknown solution");
                match $status {
                    UIExecutionStatus::Pending => sol.status = SolutionStatus::Pending,
                    UIExecutionStatus::Started { .. } => sol.status = SolutionStatus::$start_status,
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.status {
                            sol.status = SolutionStatus::$ok_status;
                        } else {
                            sol.status = SolutionStatus::Failed(format!("{} failed", $name));
                        }
                        sol.$step_result = Some(result);
                    }
                    UIExecutionStatus::Skipped => {
                        if let SolutionStatus::Failed(_) = sol.status {
                        } else {
                            sol.status = SolutionStatus::Skipped;
                        }
                    }
                }
                sol
            }};
        }

        match message {
            UIMessage::StopUI => {}
            UIMessage::ServerStatus { status } => self.executor_status = Some(status),
            UIMessage::Solutions { solutions } => {
                self.solutions = solutions
                    .into_iter()
                    .map(|info| (info.path.clone(), SolutionState::new(info)))
                    .collect();
            }
            UIMessage::Compilation { file, status } => self
                .compilations
                .entry(file)
                .or_insert(CompilationStatus::Pending)
                .apply_status(status),
            UIMessage::TerryTask { .. } => {}
            UIMessage::TerryGeneration {
                solution,
                seed,
                status,
            } => {
                let sol = process_step!(
                    self,
                    solution,
                    status,
                    generator_result,
                    Generating,
                    Generated,
                    "Generator"
                );
                sol.seed = Some(seed);
            }
            UIMessage::TerryValidation { solution, status } => {
                process_step!(
                    self,
                    solution,
                    status,
                    validator_result,
                    Validating,
                    Validated,
                    "Validator"
                );
            }
            UIMessage::TerrySolution { solution, status } => {
                process_step!(
                    self,
                    solution,
                    status,
                    solution_result,
                    Solving,
                    Solved,
                    "Solution"
                );
            }
            UIMessage::TerryChecker { solution, status } => {
                process_step!(
                    self,
                    solution,
                    status,
                    checker_result,
                    Checking,
                    Done,
                    "Checker"
                );
            }
            UIMessage::TerrySolutionOutcome { solution, outcome } => {
                let sol = self
                    .solutions
                    .get_mut(&solution)
                    .expect("Outcome of an unknown solution");
                sol.outcome = Some(outcome);
            }
            UIMessage::Warning { message } => {
                self.warnings.push(message);
            }
            UIMessage::Error { message } => {
                self.errors.push(message);
            }
            UIMessage::IOITask { .. }
            | UIMessage::IOIGeneration { .. }
            | UIMessage::IOIValidation { .. }
            | UIMessage::IOISolution { .. }
            | UIMessage::IOIEvaluation { .. }
            | UIMessage::IOIChecker { .. }
            | UIMessage::IOITestcaseScore { .. }
            | UIMessage::IOISubtaskScore { .. }
            | UIMessage::IOITaskScore { .. }
            | UIMessage::IOIBooklet { .. }
            | UIMessage::IOIBookletDependency { .. } => unreachable!("IOI message on Terry UI"),
        }
    }

    fn finish(&mut self) {
        finish_ui::FinishUI::print(self)
    }
}
