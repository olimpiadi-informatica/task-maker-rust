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
            _ => {}
        }
    }
}
