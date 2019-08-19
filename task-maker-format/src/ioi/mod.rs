//! The IOI task format.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use failure::{bail, Error};
use serde::{Deserialize, Serialize};

use task_maker_lang::GraderMap;

use crate::ui::*;
use crate::{EvaluationData, SourceFile, TaskFormat};

mod format;

/// In IOI tasks the subtask numbers are non-negative 0-based integers.
pub type SubtaskId = u32;
/// In IOI tasks the testcase numbers are non-negative 0-based integers.
pub type TestcaseId = u32;

/// Which tool to use to compute the score on a testcase given the input file, the _correct_ output
/// file and the output file to evaluate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Checker {
    /// Use a built-in white diff checker that scores 1.0 if the two output files are identical
    /// except for white spaces. It internally uses `diff --ignore-all-spaces`
    WhiteDiff,
    /// Use a custom checker based on an executable that can output a score (from 0.0 to 1.0) as
    /// well as a custom message.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// The source of the input files. It can either be a statically provided input file or a custom
/// command that will generate an input file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputGenerator {
    /// Use the static file as input. The file will be copied without transformations.
    StaticFile(PathBuf),
    /// Use a custom command to generate the input file. The file has to be printed to stdout.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// An input file validator is responsible for checking that the input file follows the format and
/// constraints defined by the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputValidator {
    /// Skip the validation and assume the input file is valid.
    AssumeValid,
    /// Use a custom command to check if the input file is valid. The command should exit with
    /// non-zero return code if the input is invalid.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// The source of the output files. It can either be a statically provided output file or a custom
/// command that will generate an output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputGenerator {
    /// Use the static file as output. The file will be copied without transformations.
    StaticFile(PathBuf),
    /// Use a custom command to generate the output file. The file has to be printed to stdout.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// The aggregator of testcase scores to compute the subtask score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestcaseScoreAggregator {
    /// Take the minimum of all the testcases, formally:
    ///
    /// `st_score = st_max_score * min(*testcase_scores)`
    Min,
    /// Sum the score of all the testcases, formally:
    ///
    /// `st_score = st_max_score * sum(*testcase_scores) / len(*testcase_scores)`
    Sum,
}

/// The type of the task. This changes the behaviour of the solutions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// The solution is a single file that will be executed once per testcase, feeding in the input
    /// file and reading the output file. The solution may be compiled with additional graders
    /// (called grader.LANG). The output is checked with an external program.
    Batch,
}

/// Information about a generic IOI task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Path of the directory of the task.
    pub path: PathBuf,
    /// The type of the task.
    pub task_type: TaskType,
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub title: String,
    /// The time limit for the execution of the solutions, if `None` it's unlimited.
    pub time_limit: Option<f64>,
    /// The memory limit in MiB of the execution of the solution, if `None` it's unlimited.
    pub memory_limit: Option<u64>,
    /// The input file for the solutions, usually `Some("input.txt")` or `None` (stdin).
    pub infile: Option<PathBuf>,
    /// The output file for the solutions, usually `Some("output.txt")` or `None` (stdout).
    pub outfile: Option<PathBuf>,
    /// The list of the subtasks.
    pub subtasks: HashMap<SubtaskId, SubtaskInfo>,
    /// The checker to use for this task.
    pub checker: Checker,
    /// The aggregator to use to compute the score of the subtask based on the score of the
    /// testcases.
    pub testcase_score_aggregator: TestcaseScoreAggregator,
    /// The graders registered for this task.
    pub grader_map: Arc<GraderMap>,
}

/// A subtask of a IOI task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskInfo {
    /// The id of the subtask.
    pub id: SubtaskId,
    /// The maximum score of the subtask, must be >= 0.
    pub max_score: f64,
    /// The testcases inside this subtask.
    pub testcases: HashMap<TestcaseId, TestcaseInfo>,
}

/// A testcase of a IOI task.
///
/// Every testcase has an input and an output that will be put in the input/ and output/ folders.
/// The files are written there only if it's not a dry-run and if the files are not static.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestcaseInfo {
    /// The id of the testcase.
    pub id: TestcaseId,
    /// The generator of the input file for this testcase.
    pub input_generator: InputGenerator,
    /// The validator of the input file for this testcase.
    pub input_validator: InputValidator,
    /// The generator of the output file for this testcase.
    pub output_generator: OutputGenerator,
}

impl Task {
    /// Try to make a `Task` from the specified path. Will return `None` if the format of the task
    /// is not IOI or if the task is corrupted and cannot be parsed.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Task, Error> {
        format::italian_yaml::parse_task(path)
    }
}

impl TaskFormat for Task {
    fn ui(&self, ui_type: UIType) -> Result<Box<dyn UI>, Error> {
        match ui_type {
            UIType::Raw => Ok(Box::new(RawUI::new())),
            UIType::Print => Ok(Box::new(PrintUI::new())),
            _ => bail!("IOI task does not current support this ui: {:?}", ui_type),
        }
    }

    fn execute(&self, eval: &mut EvaluationData) -> Result<(), Error> {
        unimplemented!()
    }
}

impl FromStr for TestcaseScoreAggregator {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "min" => Ok(TestcaseScoreAggregator::Min),
            "sum" => Ok(TestcaseScoreAggregator::Sum),
            _ => bail!("Invalid testcase score aggregator: {}", s),
        }
    }
}
