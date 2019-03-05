use crate::execution::*;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;

pub mod ioi;

/// The result of the checking process
pub struct CheckerResult {}

pub trait Generator<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    /// Generate an input file editing the DAG and returning the uuid of the
    /// file.
    fn generate(
        &self,
        dag: &mut ExecutionDAG,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> FileUuid;
}

pub trait Validator<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    /// Validate the input file editing the DAG and returing an artifact of the
    /// validator, something to keep tracks of the dependencies.
    fn validate(
        &self,
        dag: &mut ExecutionDAG,
        input: FileUuid,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> FileUuid;
}

pub trait Solution<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    /// Generate the output file editing the DAG and returning the uuid of the
    /// output file.
    fn solve(
        &self,
        dag: &mut ExecutionDAG,
        input: FileUuid,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> FileUuid;
}

pub trait Checker<SubtaskId, TestcaseId, F>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
    F: Fn(CheckerResult) -> (),
{
    /// Add the checking process to the DAG and call the callback when the
    /// checker is done
    fn check(
        &self,
        dag: &mut ExecutionDAG,
        input: FileUuid,
        output: FileUuid,
        test: FileUuid,
        subtask: SubtaskId,
        testcase: TestcaseId,
        callback: F,
    );
}

pub trait TestcaseInfo {
    /// Write the input file to this path if it's not a dry-run
    fn write_input_to(&self) -> Option<PathBuf>;

    /// Write the output file to this path if it's not a dry-run
    fn write_output_to(&self) -> Option<PathBuf>;
}

pub trait EvaluationOptions {
    /// Whether the input/output files should be written somewhere
    fn dry_run(&self) -> bool;

    /// The cache mode to use for the evaluation
    fn cache_mode(&self) -> bool;
}

pub trait Task {
    /// Type of the identifier of a subtask
    type SubtaskId: Eq + PartialOrd + Hash + Copy;
    /// Type of the identifier of a testcase
    type TestcaseId: Eq + PartialOrd + Hash + Copy;
    /// Type of the information about a testcase
    type TestcaseInfo: TestcaseInfo;

    /// Name of the format of the task
    fn format() -> &'static str;

    /// Name of the task (the short one)
    fn name(&self) -> String;

    /// Title of the task (the long one)
    fn title(&self) -> String;

    /// The list of testcases and subtasks for this task
    fn subtasks(&self) -> HashMap<Self::SubtaskId, HashMap<Self::TestcaseId, Self::TestcaseInfo>>;

    /// The list of known solution files
    fn solutions(&self) -> HashMap<PathBuf, &Solution<Self::SubtaskId, Self::TestcaseId>>;

    /// The generator that will create that testcase
    fn generator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Box<Generator<Self::SubtaskId, Self::TestcaseId>>;

    /// The optional validator that will validate that testcase
    fn validator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Validator<Self::SubtaskId, Self::TestcaseId>>>;

    /// The optional official solution that will generate the output file
    fn official_solution(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Solution<Self::SubtaskId, Self::TestcaseId>>>;

    /// The optional checker that will check the output file
    fn checker<F>(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Checker<Self::SubtaskId, Self::TestcaseId, F>>>;

    /// Starts the actual evaluation of the task
    fn evaluate<O>(&self, _options: O)
    where
        O: EvaluationOptions,
    {
        let subtasks = self.subtasks();
        for (st_num, st) in subtasks.iter() {
            for (tc_num, tc) in st.iter() {
                self.validator(*st_num, *tc_num).unwrap();
            }
        }
        unimplemented!();
    }
}
