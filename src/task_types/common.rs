use crate::task_types::*;
use std::hash::Hash;
use std::path::{Path, PathBuf};

/// A generator/solution that will simply use a static file
#[derive(Debug)]
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

/// A checker that compares the output files ignoring the whitespaces
///
/// It uses `diff --ignore-all-spaces correct test`
#[derive(Debug)]
pub struct WhiteDiffChecker;

impl WhiteDiffChecker {
    /// Make a new WhiteDiffChecker
    pub fn new() -> WhiteDiffChecker {
        WhiteDiffChecker {}
    }
}

impl<SubtaskId, TestcaseId> Generator<SubtaskId, TestcaseId> for StaticFileProvider
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    fn generate(
        &self,
        eval: &mut EvaluationData,
        _subtask: SubtaskId,
        _testcase: TestcaseId,
    ) -> (File, Option<Execution>) {
        let file = File::new(&self.description);
        eval.dag.provide_file(file.clone(), &self.path);
        (file, None)
    }
}

impl<SubtaskId, TestcaseId> Solution<SubtaskId, TestcaseId> for StaticFileProvider
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    fn solve(
        &self,
        eval: &mut EvaluationData,
        _input: File,
        _validation: Option<File>,
        _subtask: SubtaskId,
        _testcase: TestcaseId,
    ) -> (File, Option<Execution>) {
        let file = File::new(&self.description);
        eval.dag.provide_file(file.clone(), &self.path);
        (file, None)
    }
}

impl<SubtaskId, TestcaseId> Checker<SubtaskId, TestcaseId> for WhiteDiffChecker
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    fn check(
        &self,
        eval: &mut EvaluationData,
        _input: File,
        output: Option<File>,
        test: File,
        _subtask: SubtaskId,
        _testcase: TestcaseId,
        callback: Box<Fn(CheckerResult) -> ()>,
    ) {
        let output = output.expect("WhiteDiffChecker requires the input file to be present");
        let mut exec = Execution::new(
            "Whitediff checker",
            ExecutionCommand::System(PathBuf::from("diff")),
        );
        exec.args = vec![
            "--ignore-all-space".to_owned(),
            "correct".to_owned(),
            "test".to_owned(),
        ];
        exec.input(&output, Path::new("correct"), false);
        exec.input(&test, Path::new("test"), false);
        eval.dag
            .on_execution_done(&exec.uuid, move |result| match result.result.status {
                ExecutionStatus::Success => callback(CheckerResult::new(1.0, None)),
                _ => callback(CheckerResult::new(0.0, None)),
            });
        eval.dag.add_execution(exec);
    }
}
