use crate::execution::*;
use crate::ui::*;

/// The status of the compilation of a file.
pub enum CompilationStatus {
    /// The compilation is known but it has not started yet.
    Pending,
    /// The compilation is running on a worker.
    Running,
    /// The compilation has completed.
    Done,
    /// The compilation has failed.
    Failed,
    /// The compilation has been skipped.
    Skipped,
}

/// Status of the generation of a testcase input and output.
pub enum TestcaseGenerationStatus {
    /// The generation has not started yet.
    Pending,
    /// The input file is generating.
    Generating,
    /// The input file has been generated.
    Generated,
    /// The input file is being validated.
    Validating,
    /// The input file has been validated.
    Validated,
    /// The output file is generating.
    Solving,
    /// The output file has been generated.
    Solved,
    /// The generation of the testcase has failed.
    Failed,
    /// The generation has been skipped.
    Skipped,
}

/// Status of the evalution of a solution on a testcase.
pub enum TestcaseEvaluationStatus {
    /// The solution has not started yet.
    Pending,
    /// The solution is running.
    Solving,
    /// The solution exited succesfully, waiting for the checker.
    Solved,
    /// Checker is running.
    Checking,
    /// The solution scored 100% of the testcase.
    Accepted,
    /// The output is wrong.
    WrongAnswer,
    /// The solution is partially correct.
    Partial,
    /// The solution timed out.
    TimeLimitExceeded,
    /// The solution exceeded the wall time limit.
    WallTimeLimitExceeded,
    /// The solution exceeded the memory limit.
    MemoryLimitExceeded,
    /// The solution crashed.
    RuntimeError,
    /// Something went wrong.
    Failed,
    /// The evaluation has been skipped.
    Skipped,
}

/// The state of a IOI task, all the information for the UI are stored here.
pub struct IOIUIState {
    /// The name of the task.
    pub name: String,
    /// The title of the task.
    pub title: String,
    /// The path where the task is stored on disk.
    pub path: PathBuf,
    /// The maximum score of this task.
    pub max_score: f64,
    /// The list of known subtasks.
    pub subtasks: HashMap<IOISubtaskId, IOISubtaskInfo>,
    /// The list of known testcases.
    pub testcases: HashMap<IOISubtaskId, HashSet<IOITestcaseId>>,
    /// The status of the compilations.
    pub compilations: HashMap<PathBuf, CompilationStatus>,
    /// The status of the generation of the testcases.
    pub generations: HashMap<IOISubtaskId, HashMap<IOITestcaseId, TestcaseGenerationStatus>>,
    /// The status of the evaluations of the solutions.
    pub evaluations:
        HashMap<PathBuf, HashMap<IOISubtaskId, HashMap<IOITestcaseId, TestcaseEvaluationStatus>>>,
    /// The scores of the solutions.
    pub solution_scores: HashMap<PathBuf, Option<f64>>,
}

impl TestcaseEvaluationStatus {
    /// Whether the testcase evalution has completed, either successfully or not.
    pub fn has_completed(&self) -> bool {
        match self {
            TestcaseEvaluationStatus::Pending
            | TestcaseEvaluationStatus::Solving
            | TestcaseEvaluationStatus::Solved
            | TestcaseEvaluationStatus::Checking => false,
            _ => true,
        }
    }

    /// Whether the testcase evaluation has completed successfully.
    pub fn is_success(&self) -> bool {
        match self {
            TestcaseEvaluationStatus::Accepted => true,
            _ => false,
        }
    }

    /// Whether the testcase evaluation has completed with a partial score.
    pub fn is_partial(&self) -> bool {
        match self {
            TestcaseEvaluationStatus::Partial => true,
            _ => false,
        }
    }
}

impl IOIUIState {
    /// Make a new IOIUIState.
    pub fn new(task: UIMessage) -> IOIUIState {
        if let UIMessage::IOITask {
            name,
            title,
            path,
            subtasks,
            testcases,
        } = task
        {
            IOIUIState {
                name,
                title,
                path,
                max_score: subtasks.values().map(|s| s.max_score).sum(),
                subtasks,
                testcases,
                compilations: HashMap::new(),
                generations: HashMap::new(),
                evaluations: HashMap::new(),
                solution_scores: HashMap::new(),
            }
        } else {
            panic!("IOIUIState::new called with an invalid task type");
        }
    }

    /// Apply a UIMessage to this state.
    pub fn apply(&mut self, message: UIMessage) {
        match message {
            UIMessage::Compilation { file, status } => {
                let comp = self
                    .compilations
                    .entry(file.clone())
                    .or_insert(CompilationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *comp = CompilationStatus::Pending,
                    UIExecutionStatus::Started { .. } => *comp = CompilationStatus::Running,
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.result.status {
                            *comp = CompilationStatus::Done;
                        } else {
                            if self.solution_scores.contains_key(&file) {
                                *self.solution_scores.get_mut(&file).unwrap() = Some(0.0);
                            }
                            *comp = CompilationStatus::Failed;
                        }
                    }
                    UIExecutionStatus::Skipped => *comp = CompilationStatus::Skipped,
                }
            }
            UIMessage::IOIGeneration {
                subtask,
                testcase,
                status,
            } => {
                let gen = self
                    .generations
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseGenerationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *gen = TestcaseGenerationStatus::Pending,
                    UIExecutionStatus::Started { .. } => {
                        *gen = TestcaseGenerationStatus::Generating
                    }
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.result.status {
                            *gen = TestcaseGenerationStatus::Generated;
                        } else {
                            *gen = TestcaseGenerationStatus::Failed;
                        }
                    }
                    UIExecutionStatus::Skipped => *gen = TestcaseGenerationStatus::Skipped,
                }
            }
            UIMessage::IOIValidation {
                subtask,
                testcase,
                status,
            } => {
                let gen = self
                    .generations
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseGenerationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *gen = TestcaseGenerationStatus::Pending,
                    UIExecutionStatus::Started { .. } => {
                        *gen = TestcaseGenerationStatus::Validating
                    }
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.result.status {
                            *gen = TestcaseGenerationStatus::Validated;
                        } else {
                            *gen = TestcaseGenerationStatus::Failed;
                        }
                    }
                    UIExecutionStatus::Skipped => {
                        if let TestcaseGenerationStatus::Failed = *gen {
                        } else {
                            *gen = TestcaseGenerationStatus::Skipped;
                        }
                    }
                }
            }
            UIMessage::IOISolution {
                subtask,
                testcase,
                status,
            } => {
                let gen = self
                    .generations
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseGenerationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *gen = TestcaseGenerationStatus::Pending,
                    UIExecutionStatus::Started { .. } => *gen = TestcaseGenerationStatus::Solving,
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.result.status {
                            *gen = TestcaseGenerationStatus::Solved;
                        } else {
                            *gen = TestcaseGenerationStatus::Failed;
                        }
                    }
                    UIExecutionStatus::Skipped => {
                        if let TestcaseGenerationStatus::Failed = *gen {
                        } else {
                            *gen = TestcaseGenerationStatus::Skipped;
                        }
                    }
                }
            }
            UIMessage::IOIEvaluation {
                subtask,
                testcase,
                solution,
                status,
            } => {
                self.solution_scores.entry(solution.clone()).or_default();
                let eval = self
                    .evaluations
                    .entry(solution)
                    .or_default()
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseEvaluationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *eval = TestcaseEvaluationStatus::Pending,
                    UIExecutionStatus::Started { .. } => *eval = TestcaseEvaluationStatus::Solving,
                    UIExecutionStatus::Done { result } => match result.result.status {
                        ExecutionStatus::Success => *eval = TestcaseEvaluationStatus::Solved,
                        ExecutionStatus::ReturnCode(_) => {
                            *eval = TestcaseEvaluationStatus::RuntimeError
                        }
                        ExecutionStatus::Signal(_, _) => {
                            *eval = TestcaseEvaluationStatus::RuntimeError
                        }
                        ExecutionStatus::TimeLimitExceeded => {
                            *eval = TestcaseEvaluationStatus::TimeLimitExceeded
                        }
                        ExecutionStatus::SysTimeLimitExceeded => {
                            *eval = TestcaseEvaluationStatus::TimeLimitExceeded
                        }
                        ExecutionStatus::WallTimeLimitExceeded => {
                            *eval = TestcaseEvaluationStatus::WallTimeLimitExceeded
                        }
                        ExecutionStatus::MemoryLimitExceeded => {
                            *eval = TestcaseEvaluationStatus::MemoryLimitExceeded
                        }
                        ExecutionStatus::InternalError(_) => {
                            *eval = TestcaseEvaluationStatus::Failed
                        }
                    },
                    UIExecutionStatus::Skipped => *eval = TestcaseEvaluationStatus::Skipped,
                }
            }
            UIMessage::IOIChecker {
                subtask,
                testcase,
                solution,
                status,
            } => {
                let eval = self
                    .evaluations
                    .entry(solution)
                    .or_default()
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseEvaluationStatus::Pending);
                if let UIExecutionStatus::Started { .. } = status {
                    *eval = TestcaseEvaluationStatus::Checking;
                }
            }
            UIMessage::IOITestcaseScore {
                subtask,
                testcase,
                solution,
                score,
            } => {
                let eval = self
                    .evaluations
                    .entry(solution)
                    .or_default()
                    .entry(subtask)
                    .or_default()
                    .entry(testcase)
                    .or_insert(TestcaseEvaluationStatus::Pending);
                if let TestcaseEvaluationStatus::Checking = eval {
                    if score == 0.0 {
                        *eval = TestcaseEvaluationStatus::WrongAnswer;
                    } else if (score - 1.0).abs() < 0.001 {
                        *eval = TestcaseEvaluationStatus::Accepted;
                    } else {
                        *eval = TestcaseEvaluationStatus::Partial;
                    }
                }
            }
            UIMessage::IOITaskScore { solution, score } => {
                *self.solution_scores.entry(solution).or_default() = Some(score);
            }
            _ => {}
        }
    }
}
