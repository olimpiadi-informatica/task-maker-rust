use crate::format::ioi::*;
use crate::format::*;

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

impl<F> Checker<IOISubtaskId, IOITestcaseId, F> for IOIBatchChecker
where
    F: Fn(CheckerResult) -> () + 'static,
{
    fn check(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        output: Option<File>,
        test: File,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
        callback: F,
    ) {
        unimplemented!();
    }
}

impl Task for IOIBatchTask {
    type SubtaskId = IOISubtaskId;
    type TestcaseId = IOITestcaseId;
    type SubtaskInfo = IOISubtaskInfo;
    type TestcaseInfo = IOITestcaseInfo;

    fn format() -> &'static str {
        "ioi-batch"
    }

    fn name(&self) -> String {
        unimplemented!();
    }

    fn title(&self) -> String {
        unimplemented!();
    }

    fn subtasks(&self) -> HashMap<Self::SubtaskId, Self::SubtaskInfo> {
        unimplemented!();
    }

    fn testcases(&self, subtask: Self::SubtaskId) -> HashMap<Self::TestcaseId, Self::TestcaseInfo> {
        unimplemented!();
    }

    fn solutions(&self) -> HashMap<PathBuf, &Solution<Self::SubtaskId, Self::TestcaseId>> {
        unimplemented!();
    }

    fn generator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Box<Generator<Self::SubtaskId, Self::TestcaseId>> {
        Box::new(StaticFileProvider::new(
            format!("Static input of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        ))
    }

    fn validator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Validator<Self::SubtaskId, Self::TestcaseId>>> {
        Some(Box::new(IOIBatchValidator {}))
    }

    fn official_solution(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Solution<Self::SubtaskId, Self::TestcaseId>>> {
        Some(Box::new(StaticFileProvider::new(
            format!("Static output of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        )))
    }

    fn checker<F>(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Box<Checker<Self::SubtaskId, Self::TestcaseId, F>> {
        unimplemented!();
    }
}
