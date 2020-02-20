use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use task_maker_dag::{ExecutionResult, ExecutionStatus};
use task_maker_exec::ExecutorStatus;

use crate::terry::{Seed, SolutionOutcome, Task};
use crate::ui::{CompilationStatus, UIExecutionStatus, UIMessage, UIStateT};

/// The state of a Terry task, all the information for the UI are stored here.
#[derive(Debug, Clone)]
pub struct UIState {
    /// The task.
    pub task: Task,
    /// The status of the compilations.
    pub compilations: HashMap<PathBuf, CompilationStatus>,
    /// The state of the solutions known.
    pub solutions: HashMap<PathBuf, SolutionState>,
    /// The status of the executor.
    pub executor_status: Option<ExecutorStatus<SystemTime>>,
    /// All the emitted warnings.
    pub warnings: Vec<String>,
}

/// The state of the evaluation of a solution.
#[derive(Debug, Clone, Default)]
pub struct SolutionState {
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

impl UIState {
    /// Make a new `UIState`.
    pub fn new(task: &Task) -> UIState {
        UIState {
            task: task.clone(),
            compilations: HashMap::new(),
            solutions: HashMap::new(),
            executor_status: None,
            warnings: Vec::new(),
        }
    }
}

impl UIStateT for UIState {
    /// Apply a `UIMessage` to this state.
    fn apply(&mut self, message: UIMessage) {
        macro_rules! process_step {
            ($self:expr, $solution:expr, $status:expr, $step_result:tt, $start_status:tt, $ok_status:tt, $name:literal) => {{
                let sol = $self.solutions.entry($solution).or_default();
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
            UIMessage::Compilation { file, status } => self
                .compilations
                .entry(file)
                .or_insert(CompilationStatus::Pending)
                .apply_status(status),
            UIMessage::CompilationStdout { file, content } => self
                .compilations
                .entry(file)
                .or_insert(CompilationStatus::Pending)
                .apply_stdout(content),
            UIMessage::CompilationStderr { file, content } => self
                .compilations
                .entry(file)
                .or_insert(CompilationStatus::Pending)
                .apply_stderr(content),
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
                let sol = self.solutions.entry(solution).or_default();
                sol.outcome = Some(outcome);
            }
            UIMessage::Warning { message } => {
                self.warnings.push(message);
            }
            UIMessage::IOITask { .. }
            | UIMessage::IOIGeneration { .. }
            | UIMessage::IOIGenerationStderr { .. }
            | UIMessage::IOIValidation { .. }
            | UIMessage::IOIValidationStderr { .. }
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
}
