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

/// The state of a IOI task, all the information for the UI are stored here.
pub struct IOIUIState {
    /// The name of the task.
    pub name: String,
    /// The title of the task.
    pub title: String,
    /// The path where the task is stored on disk.
    pub path: PathBuf,
    /// The list of known subtasks.
    pub subtasks: HashMap<IOISubtaskId, IOISubtaskInfo>,
    /// The list of known testcases.
    pub testcases: HashMap<IOISubtaskId, HashSet<IOITestcaseId>>,
    /// The status of the compilations.
    pub compilations: HashMap<PathBuf, CompilationStatus>,
    /// The status of the generation of the testcases.
    pub generations: HashMap<IOISubtaskId, HashMap<IOITestcaseId, TestcaseGenerationStatus>>,
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
                subtasks,
                testcases,
                compilations: HashMap::new(),
                generations: HashMap::new(),
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
                    .entry(file)
                    .or_insert(CompilationStatus::Pending);
                match status {
                    UIExecutionStatus::Pending => *comp = CompilationStatus::Pending,
                    UIExecutionStatus::Started { .. } => *comp = CompilationStatus::Running,
                    UIExecutionStatus::Done { result } => {
                        if let ExecutionStatus::Success = result.result.status {
                            *comp = CompilationStatus::Done;
                        } else {
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
            _ => {}
        }
    }
}
