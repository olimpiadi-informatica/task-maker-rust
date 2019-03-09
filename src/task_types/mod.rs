use crate::execution::*;
use crate::score_types::*;
use failure::Error;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
pub trait Generator<SubtaskId, TestcaseId>: std::fmt::Debug
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
pub trait Validator<SubtaskId, TestcaseId>: std::fmt::Debug
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
pub trait Solution<SubtaskId, TestcaseId>: std::fmt::Debug
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
pub trait Checker<SubtaskId, TestcaseId>: std::fmt::Debug
where
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
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
        callback: Box<Fn(CheckerResult) -> ()>,
    );
}

/// Some basic information about a subtask
pub trait SubtaskInfo {
    /// Maximum score of this subtask
    fn max_score(&self) -> f64;
}

/// Some basic information about a testcase.
pub trait TestcaseInfo<
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
>
{
    /// Write the input file to this path if it's not a dry-run, relative to
    /// the task directory.
    fn write_input_to(&self) -> Option<PathBuf>;

    /// Write the output file to this path if it's not a dry-run, relative to
    /// the task directory.
    fn write_output_to(&self) -> Option<PathBuf>;

    /// The generator that will create that testcase
    fn generator(&self) -> Arc<Generator<SubtaskId, TestcaseId>>;

    /// The optional validator that will validate that testcase
    fn validator(&self) -> Option<Arc<Validator<SubtaskId, TestcaseId>>>;
}

/// The options for an evaluation
pub trait EvaluationOptions {
    /// Whether the input/output files should be written somewhere
    fn dry_run(&self) -> bool;

    /// The cache mode to use for the evaluation
    fn cache_mode(&self) -> bool;
}

/// Trait that describes a generic task. Every task must have a generator (a
/// way of getting testcases) and can have a validator, an official solution,
/// but has to have a checker that assigns a score to a solution.
pub trait Task<
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
    SubtaskInfo: crate::task_types::SubtaskInfo,
    TestcaseInfo: crate::task_types::TestcaseInfo<SubtaskId, TestcaseId>,
>: std::fmt::Debug
{
    /// Name of the format of the task
    fn format() -> &'static str
    where
        Self: Sized;

    /// Path to the root folder of the task.
    fn path(&self) -> &Path;

    /// Name of the task (the short one)
    fn name(&self) -> String;

    /// Title of the task (the long one)
    fn title(&self) -> String;

    /// The list of the subtasks for this task
    fn subtasks(&self) -> &HashMap<SubtaskId, SubtaskInfo>;

    /// The list of the testcases for that subtask
    fn testcases(&self, subtask: SubtaskId) -> &HashMap<TestcaseId, TestcaseInfo>;

    /// The score type to use for this task.
    fn score_type(&self) -> &ScoreType<SubtaskId, TestcaseId>;

    /// The list of known solution files
    fn solutions(&self) -> &HashMap<PathBuf, Box<Solution<SubtaskId, TestcaseId>>>;

    /// The optional official solution that will generate the output file
    fn official_solution(
        &self,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> &Option<Box<Solution<SubtaskId, TestcaseId>>>;

    /// The optional checker that will check the output file
    fn checker(
        &self,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> &Box<Checker<SubtaskId, TestcaseId>>;

    /// Build the DAG of the evaluation of this task.
    fn evaluate(&self, dag: &mut ExecutionDAG, options: &EvaluationOptions) {
        let subtasks = self.subtasks();
        let mut inputs = HashMap::new();
        let mut outputs = HashMap::new();
        let solutions = self.solutions();
        // TODO register the scores
        // let solutions_scores: HashMap<PathBuf, Box<dyn ScoreType<SubtaskId, TestcaseId>>> =
        //     solutions
        //         .keys()
        //         .map(|sol| (sol.clone(), self.score_type().clone()))
        //         .collect();
        for (st_num, _st) in subtasks.iter() {
            inputs.insert(*st_num, HashMap::new());
            outputs.insert(*st_num, HashMap::new());
            for (tc_num, tc) in self.testcases(*st_num).iter() {
                let input = tc.generator().generate(dag, *st_num, *tc_num);
                if let Some(path) = tc.write_input_to() {
                    if !options.dry_run() {
                        dag.write_file_to(&input, &self.path().join(path));
                    }
                }
                let val = if let Some(validator) = tc.validator() {
                    Some(validator.validate(dag, input.clone(), *st_num, *tc_num))
                } else {
                    None
                };
                let output = if let Some(solution) = self.official_solution(*st_num, *tc_num) {
                    Some(solution.solve(
                        dag,
                        input.clone(),
                        val.as_ref().map(|f| f.clone()),
                        *st_num,
                        *tc_num,
                    ))
                } else {
                    None
                };
                if let (Some(ref output), Some(ref path)) = (&output, &tc.write_output_to()) {
                    if !options.dry_run() {
                        dag.write_file_to(&output, &self.path().join(path));
                    }
                }

                inputs
                    .get_mut(&st_num)
                    .unwrap()
                    .insert(*tc_num, input.clone());
                outputs
                    .get_mut(&st_num)
                    .unwrap()
                    .insert(*tc_num, output.clone());

                for (_sol_path, sol) in solutions.iter() {
                    let sol_output = sol.solve(dag, input.clone(), val.clone(), *st_num, *tc_num);
                    self.checker(*st_num, *tc_num).check(
                        dag,
                        input.clone(),
                        output.clone(),
                        sol_output,
                        *st_num,
                        *tc_num,
                        Box::new(|_res| {
                            // TODO register the score!
                        }),
                    );
                }
            }
        }
    }
}

/// A task format is a way of laying files in a task folder, every task folder
/// contains a single task which type can be different even for the same
/// format. For example in a IOI-like format there could be a Batch task, a
/// Communication task, ...
pub trait TaskFormat {
    /// Type of the identifier of a subtask
    type SubtaskId: Eq + PartialOrd + Hash + Copy;
    /// Type of the identifier of a testcase
    type TestcaseId: Eq + PartialOrd + Hash + Copy;
    /// Type of the information about a subtask
    type SubtaskInfo: SubtaskInfo;
    /// Type of the information about a testcase
    type TestcaseInfo: TestcaseInfo<Self::SubtaskId, Self::TestcaseId>;

    /// Whether the `path` points to a valid task for this format.
    fn is_valid(path: &Path) -> bool;

    /// Assuming `path` is valid make a Task from that directory.
    fn parse(
        path: &Path,
    ) -> Result<
        Box<Task<Self::SubtaskId, Self::TestcaseId, Self::SubtaskInfo, Self::TestcaseInfo>>,
        Error,
    >;
}
