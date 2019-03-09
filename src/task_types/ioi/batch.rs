use crate::task_types::ioi::*;
use crate::task_types::*;

#[derive(Debug)]
pub struct IOIBatchTask {
    pub info: IOITaskInfo,
}

impl Task<IOISubtaskId, IOITestcaseId, IOISubtaskInfo, IOITestcaseInfo> for IOIBatchTask {
    fn format() -> &'static str {
        "ioi-batch"
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

    fn solutions(&self) -> HashMap<PathBuf, &Solution<IOISubtaskId, IOITestcaseId>> {
        unimplemented!();
    }

    fn official_solution(
        &self,
        _subtask: IOISubtaskId,
        _testcase: IOITestcaseId,
    ) -> Option<Box<Solution<IOISubtaskId, IOITestcaseId>>> {
        None
    }

    fn checker(
        &self,
        _subtask: IOISubtaskId,
        _testcase: IOITestcaseId,
    ) -> &Box<Checker<IOISubtaskId, IOITestcaseId>> {
        &self.info.checker
    }
}
