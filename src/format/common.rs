use crate::execution::*;
use crate::format::*;
use std::hash::Hash;
use std::path::PathBuf;

/// A generator/solution that will simply use a static file
pub struct StaticFileProvider {
    /// A textual description of the testcase, for example
    ///   'Sample input for case 0'
    pub description: String,
    /// Path to the file on the disk
    pub path: PathBuf,
}

impl StaticFileProvider {
    /// Make a new StaticFileProvider
    pub fn new(description: String, path: PathBuf) -> StaticFileProvider {
        StaticFileProvider { description, path }
    }
}

impl<SubtaskId, TestcaseId> Generator<SubtaskId, TestcaseId> for StaticFileProvider
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    fn generate(&self, dag: &mut ExecutionDAG, _subtask: SubtaskId, _testcase: TestcaseId) -> File {
        let file = File::new(&self.description);
        dag.provide_file(file.clone(), &self.path);
        file
    }
}

impl<SubtaskId, TestcaseId> Solution<SubtaskId, TestcaseId> for StaticFileProvider
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    fn solve(
        &self,
        dag: &mut ExecutionDAG,
        _input: FileUuid,
        _validation: Option<FileUuid>,
        _subtask: SubtaskId,
        _testcase: TestcaseId,
    ) -> File {
        let file = File::new(&self.description);
        dag.provide_file(file.clone(), &self.path);
        file
    }
}
