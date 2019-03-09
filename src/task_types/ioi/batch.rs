use crate::task_types::ioi::*;
use crate::task_types::*;

#[derive(Debug)]
pub struct IOIBatchTask {
    pub info: IOITaskInfo,
}

pub struct IOIBatchValidator;

pub struct IOIBatchSolution;

pub struct IOIBatchChecker;

impl Validator<IOISubtaskId, IOITestcaseId> for IOIBatchValidator {
    fn validate(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> File {
        unimplemented!();
    }
}

impl Solution<IOISubtaskId, IOITestcaseId> for IOIBatchSolution {
    fn solve(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        validation: Option<File>,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> File {
        unimplemented!();
    }
}

impl Checker<IOISubtaskId, IOITestcaseId> for IOIBatchChecker {
    fn check(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        output: Option<File>,
        test: File,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        callback: Box<Fn(CheckerResult) -> ()>,
    ) {
        unimplemented!();
    }
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

    fn generator(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> &Box<Generator<IOISubtaskId, IOITestcaseId>> {
        &self
            .info
            .testcases
            .get(&subtask)
            .unwrap()
            .get(&testcase)
            .unwrap()
            .generator
    }

    fn validator(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Option<Box<Validator<IOISubtaskId, IOITestcaseId>>> {
        None
    }

    fn official_solution(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Option<Box<Solution<IOISubtaskId, IOITestcaseId>>> {
        None
    }

    fn checker(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Box<Checker<IOISubtaskId, IOITestcaseId>> {
        unimplemented!();
    }
}
