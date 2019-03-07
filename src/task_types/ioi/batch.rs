use crate::task_types::ioi::*;
use crate::task_types::*;

pub struct IOIBatchTask {
    pub info: IOITaskInfo,
}

pub struct IOIBatchGenerator;

pub struct IOIBatchValidator;

pub struct IOIBatchSolution;

pub struct IOIBatchChecker;

impl Generator<IOISubtaskId, IOITestcaseId> for IOIBatchGenerator {
    fn generate(
        &self,
        dag: &mut ExecutionDAG,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> File {
        unimplemented!();
    }
}

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
        unimplemented!();
    }

    fn title(&self) -> String {
        unimplemented!();
    }

    fn subtasks(&self) -> HashMap<IOISubtaskId, IOISubtaskInfo> {
        unimplemented!();
    }

    fn testcases(&self, subtask: IOISubtaskId) -> HashMap<IOITestcaseId, IOITestcaseInfo> {
        unimplemented!();
    }

    fn solutions(&self) -> HashMap<PathBuf, &Solution<IOISubtaskId, IOITestcaseId>> {
        unimplemented!();
    }

    fn generator(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Box<Generator<IOISubtaskId, IOITestcaseId>> {
        Box::new(StaticFileProvider::new(
            format!("Static input of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        ))
    }

    fn validator(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Option<Box<Validator<IOISubtaskId, IOITestcaseId>>> {
        Some(Box::new(IOIBatchValidator {}))
    }

    fn official_solution(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Option<Box<Solution<IOISubtaskId, IOITestcaseId>>> {
        Some(Box::new(StaticFileProvider::new(
            format!("Static output of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        )))
    }

    fn checker(
        &self,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> Box<Checker<IOISubtaskId, IOITestcaseId>> {
        unimplemented!();
    }
}
