use crate::task_types::ioi::*;
use crate::task_types::*;

/// In a IOI Batch task each input file is evaluated by a single solution which
/// takes the input file as an input (at stdin or at a file path) and writes
/// the output file (to stdout or at a file path).
///
/// The output is then checked by a checker which takes the input, the output
/// and the correct output (produced by the official solution).
#[derive(Debug)]
pub struct IOIBatchTask {
    /// Information about the task
    pub info: IOITaskInfo,
    /// List of the known solutions
    pub solutions: HashMap<PathBuf, Box<Solution<IOISubtaskId, IOITestcaseId>>>,
    /// The official solution, may be None if the task has static outputs.
    pub official_solution: Option<Box<Solution<IOISubtaskId, IOITestcaseId>>>,
}

/// The interface between a IOI task and the UI.
pub struct IOITaskUIInterface;

impl Task<IOISubtaskId, IOITestcaseId, IOISubtaskInfo, IOITestcaseInfo> for IOIBatchTask {
    fn format() -> &'static str {
        "ioi-batch"
    }

    fn path(&self) -> &Path {
        &self.info.path
    }

    fn name(&self) -> String {
        self.info.yaml.name.clone()
    }

    fn title(&self) -> String {
        self.info.yaml.title.clone()
    }

    fn subtasks(&self) -> &HashMap<IOISubtaskId, IOISubtaskInfo> {
        &self.info.subtasks
    }

    fn testcases(&self, subtask: IOISubtaskId) -> &HashMap<IOITestcaseId, IOITestcaseInfo> {
        self.info.testcases.get(&subtask).unwrap()
    }

    fn score_type(&self) -> &ScoreType<IOISubtaskId, IOITestcaseId> {
        self.info.score_type.as_ref()
    }

    fn solutions(&self) -> &HashMap<PathBuf, Box<Solution<IOISubtaskId, IOITestcaseId>>> {
        &self.solutions
    }

    fn official_solution(
        &self,
        _subtask: IOISubtaskId,
        _testcase: IOITestcaseId,
    ) -> &Option<Box<Solution<IOISubtaskId, IOITestcaseId>>> {
        &self.official_solution
    }

    fn checker(
        &self,
        _subtask: IOISubtaskId,
        _testcase: IOITestcaseId,
    ) -> &Box<Checker<IOISubtaskId, IOITestcaseId>> {
        &self.info.checker
    }

    fn get_ui_interface(&self) -> Arc<TaskUIInterface<IOISubtaskId, IOITestcaseId>> {
        Arc::new(IOITaskUIInterface {})
    }
}

impl TaskUIInterface<IOISubtaskId, IOITestcaseId> for IOITaskUIInterface {
    fn generation_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        status: UIExecutionStatus,
    ) {
        sender
            .send(UIMessage::IOIGeneration {
                subtask,
                testcase,
                status,
            })
            .unwrap();
    }

    fn validation_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        status: UIExecutionStatus,
    ) {
        sender
            .send(UIMessage::IOIValidation {
                subtask,
                testcase,
                status,
            })
            .unwrap();
    }

    fn solution_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        status: UIExecutionStatus,
    ) {
        sender
            .send(UIMessage::IOISolution {
                subtask,
                testcase,
                status,
            })
            .unwrap();
    }

    fn evaluation_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        solution: PathBuf,
        status: UIExecutionStatus,
    ) {
        sender
            .send(UIMessage::IOIEvaluation {
                subtask,
                testcase,
                solution,
                status,
            })
            .unwrap();
    }

    fn checker_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        solution: PathBuf,
        status: UIExecutionStatus,
    ) {
        sender
            .send(UIMessage::IOIChecker {
                subtask,
                testcase,
                solution,
                status,
            })
            .unwrap();
    }
}
