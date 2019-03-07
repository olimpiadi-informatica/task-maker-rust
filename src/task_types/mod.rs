use crate::execution::*;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;

pub mod common;
pub mod ioi;

pub use common::*;

/// The result of the checking process
pub struct CheckerResult {
    /// Value from 0.0 (not correct) to 1.0 (correct) with the score of the
    /// solution
    pub score: f64,
    /// Optional message from the checker
    pub message: Option<String>,
}

impl CheckerResult {
    /// Make a new CheckerResult
    pub fn new(score: f64, message: Option<&str>) -> CheckerResult {
        CheckerResult {
            score,
            message: message.map(|s| s.to_owned()),
        }
    }
}

/// A trait that describes what is a generator: something that knowing which
/// testcase produces an input file
pub trait Generator<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
{
    /// Generate an input file editing the DAG and returning the uuid of the
    /// file.
    fn generate(&self, dag: &mut ExecutionDAG, subtask: SubtaskId, testcase: TestcaseId) -> File;
}

/// A trait that describes what is a validator: something that known which
/// testcase and given that input file checks if it respects all
/// constraints.
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
        input: File,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> File;
}

/// A trait that describes what is a solution: something that given an input
/// file produces an output file. An extra parameter `validation` is supplied
/// to make sure that the validation (if any) comes before.
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
        input: File,
        validation: Option<File>,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> File;
}

/// A trait that describes what is a checker: something that given an input
/// file, an optional correct output file and the contestant's output file
/// computes a score (and eventually message) for that testcase.
pub trait Checker<SubtaskId, TestcaseId, F>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
    F: Fn(CheckerResult) -> () + 'static,
{
    /// Add the checking process to the DAG and call the callback when the
    /// checker is done
    fn check(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        output: Option<File>,
        test: File,
        subtask: SubtaskId,
        testcase: TestcaseId,
        // TODO maybe tell the checker which solution it is checking
        callback: F,
    );
}

/// Some basic information about a subtask
pub trait SubtaskInfo {
    /// Maximum score of this subtask
    fn max_score(&self) -> f64;

    /// Score mode of this subtask
    fn score_mode(&self) -> String;
}

/// Some basic information about a testcase.
pub trait TestcaseInfo {
    /// Write the input file to this path if it's not a dry-run
    fn write_input_to(&self) -> Option<PathBuf>;

    /// Write the output file to this path if it's not a dry-run
    fn write_output_to(&self) -> Option<PathBuf>;
}

/// The options for an evaluation
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
    /// Type of the information about a subtask
    type SubtaskInfo: SubtaskInfo;
    /// Type of the information about a testcase
    type TestcaseInfo: TestcaseInfo;

    /// Name of the format of the task
    fn format() -> &'static str;

    /// Name of the task (the short one)
    fn name(&self) -> String;

    /// Title of the task (the long one)
    fn title(&self) -> String;

    /// The list of the subtasks for this task
    fn subtasks(&self) -> HashMap<Self::SubtaskId, Self::SubtaskInfo>;

    /// The list of the testcases for that subtask
    fn testcases(&self, subtask: Self::SubtaskId) -> HashMap<Self::TestcaseId, Self::TestcaseInfo>;

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
    ) -> Box<Checker<Self::SubtaskId, Self::TestcaseId, F>>;

    /// Starts the actual evaluation of the task
    fn evaluate<O>(&self, dag: &mut ExecutionDAG, options: O)
    where
        O: EvaluationOptions,
    {
        let subtasks = self.subtasks();
        let mut inputs = HashMap::new();
        let mut outputs = HashMap::new();
        for (st_num, st) in subtasks.iter() {
            inputs.insert(*st_num, HashMap::new());
            outputs.insert(*st_num, HashMap::new());
            for (tc_num, tc) in self.testcases(*st_num).iter() {
                let input = self
                    .generator(*st_num, *tc_num)
                    .generate(dag, *st_num, *tc_num);
                if let Some(path) = tc.write_input_to() {
                    if !options.dry_run() {
                        dag.write_file_to(&input, &path);
                    }
                }
                let val = if let Some(validator) = self.validator(*st_num, *tc_num) {
                    Some(validator.validate(dag, input.clone(), *st_num, *tc_num))
                } else {
                    None
                };
                let output = if let Some(solution) = self.official_solution(*st_num, *tc_num) {
                    Some(solution.solve(
                        dag,
                        input.clone(),
                        val.map(|f| f.clone()),
                        *st_num,
                        *tc_num,
                    ))
                } else {
                    None
                };
                if let (Some(ref output), Some(ref path)) = (&output, &tc.write_output_to()) {
                    if !options.dry_run() {
                        dag.write_file_to(&output, &path);
                    }
                }

                inputs.get_mut(&st_num).unwrap().insert(*tc_num, input);
                outputs.get_mut(&st_num).unwrap().insert(*tc_num, output);

                // TODO evaluate the submissions!
            }
        }
        unimplemented!();
    }
}
