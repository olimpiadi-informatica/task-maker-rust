//! The IOI task format.
//!
//! In IOI-like tasks there is the concept of _subtask_ and of _testcase_: a testcase is a single
//! instance of the evaluation of a solution on a given input file, producing a single output file
//! which will be used for the scoring. For each solution every testcase is worth from 0.0 to 1.0
//! points.
//!
//! A subtask is a group of testcases, it has a `max_score` parameter which scales its value from
//! 0.0 to `max_score` points. For computing the score of the subtask a `TestcaseScoreAggregator` is
//! used. The score of the task for a solution is the sum of all the subtask scores.
//!
//! There are many different valid task types, the most common is `Batch` where the solution is
//! simply executed once per testcase, feeding in the input file (either via stdin or normal file)
//! and getting the output file (either via stdout or normal file). The output is then checked using
//! a `Checker`, a program that computes the score of the testcase given the input file, the output
//! file and the _correct_ output file (the one produced by the jury).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use failure::{bail, Error};
use serde::{Deserialize, Serialize};

use task_maker_lang::GraderMap;

use crate::ui::*;
use crate::{list_files, EvaluationData, SourceFile, TaskFormat};
use crate::{EvaluationConfig, UISender};

mod curses_ui;
mod dag;
mod finish_ui;
mod format;
mod tag;
mod ui_state;

use curses_ui::CursesUI;
pub use dag::*;
use itertools::Itertools;
use std::ops::Deref;
pub use tag::*;

/// In IOI tasks the subtask numbers are non-negative 0-based integers.
pub type SubtaskId = u32;
/// In IOI tasks the testcase numbers are non-negative 0-based integers.
pub type TestcaseId = u32;

/// This struct will manage the scores of a solution in a task and will emit the ui messages when
/// a new score is ready.
#[derive(Debug, Clone)]
pub(crate) struct ScoreManager {
    /// The scores of each subtask.
    subtask_scores: HashMap<SubtaskId, Option<f64>>,
    /// The maximum score of each subtask.
    max_subtask_scores: HashMap<SubtaskId, f64>,
    /// The scores of each testcase.
    testcase_scores: HashMap<SubtaskId, HashMap<TestcaseId, Option<f64>>>,
    /// The aggregator to use for computing the subtask scores.
    aggregator: TestcaseScoreAggregator,
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
    /// Try to make a `Task` from the specified path. Will return `Err` if the format of the task
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
            UIType::Curses => Ok(Box::new(CursesUI::new(self)?)),
        }
    }

    fn execute(&self, eval: &mut EvaluationData, config: &EvaluationConfig) -> Result<(), Error> {
        let graders: HashSet<PathBuf> = self
            .grader_map
            .all_paths()
            .map(|p| p.to_path_buf())
            .collect();
        let empty_score_manager = ScoreManager::new(&self);
        let filter = config
            .solution_filter
            .iter()
            .map(|filter| {
                // unfortunate lossy cast to String because currently OsString doesn't support .starts_with
                PathBuf::from(filter)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect_vec();
        let solutions: Vec<_> = list_files(&self.path, vec!["sol/*"])
            .into_iter()
            .filter(|p| !graders.contains(p)) // the graders are not solutions
            .filter(|p| {
                if config.solution_filter.is_empty() {
                    return true;
                }
                let name = p.file_name().unwrap().to_string_lossy();
                filter.iter().any(|filter| name.starts_with(filter.deref()))
            })
            .map(|p| {
                SourceFile::new(
                    &p,
                    Some(self.grader_map.clone()),
                    Some(
                        self.path
                            .join("bin")
                            .join("sol")
                            .join(p.file_name().unwrap()),
                    ),
                )
            })
            .filter(Option::is_some) // ignore the unknown languages
            .map(Option::unwrap)
            .map(|source| (source, Arc::new(Mutex::new(empty_score_manager.clone()))))
            .collect();

        for subtask in self.subtasks.values() {
            trace!("Executing the generation of subtask {}", subtask.id);

            for testcase in subtask.testcases.values() {
                trace!(
                    "Executing the generation of testcase {} of subtask {}",
                    testcase.id,
                    subtask.id
                );

                let input =
                    testcase
                        .input_generator
                        .generate(&self, eval, subtask.id, testcase.id)?;
                let val_handle =
                    testcase
                        .input_validator
                        .validate(eval, subtask.id, testcase.id, input)?;
                let output = testcase.output_generator.generate(
                    &self,
                    eval,
                    subtask.id,
                    testcase.id,
                    input,
                    val_handle,
                )?;

                for (solution, score_manager) in solutions.iter() {
                    trace!(
                        "Evaluation of the solution {:?} against subtask {} / testcase {}",
                        solution.name(),
                        subtask.id,
                        testcase.id
                    );

                    self.task_type.evaluate(
                        &self,
                        eval,
                        subtask.id,
                        testcase.id,
                        solution,
                        input,
                        val_handle,
                        output,
                        score_manager.clone(),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn clean(&self) -> Result<(), Error> {
        let has_input_generator = self
            .subtasks
            .values()
            .flat_map(|st| st.testcases.values())
            .any(|tc| match tc.input_generator {
                InputGenerator::Custom(_, _) => true,
                _ => false,
            });
        // remove the input/ and output/ folders
        if has_input_generator {
            for dir in &["input", "output"] {
                let dir = self.path.join(dir);
                if !dir.exists() {
                    continue;
                }
                for file in glob::glob(dir.join("*.txt").to_str().unwrap()).unwrap() {
                    match file {
                        Ok(file) => {
                            info!("Removing {:?}", file);
                            std::fs::remove_file(file)?;
                        }
                        _ => warn!("Cannot process {:?}", file),
                    }
                }
                info!("Removing {:?}", dir);
                if let Err(e) = std::fs::remove_dir(&dir) {
                    if let std::io::ErrorKind::Other = e.kind() {
                        warn!("Directory {:?} not empty!", dir);
                    } else {
                        panic!("Cannot remove {:?}: {:?}", dir, e);
                    }
                }
            }
        }
        // remove the bin/ folder
        let bin_path = self.path.join("bin");
        if bin_path.exists() {
            info!("Removing {:?}", bin_path);
            std::fs::remove_dir_all(bin_path)?;
        }
        // remove the compiled checkers
        if let Checker::Custom(_) = self.checker {
            for checker in &["check/checker", "cor/correttore"] {
                let path = self.path.join(checker);
                if path.exists() {
                    info!("Removing {:?}", path);
                    std::fs::remove_file(path)?;
                }
            }
        }
        Ok(())
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

impl ScoreManager {
    /// Make a new `ScoreManager` based on the subtasks and testcases of the specified task.
    pub fn new(task: &Task) -> ScoreManager {
        ScoreManager {
            subtask_scores: task.subtasks.keys().map(|st| (*st, None)).collect(),
            max_subtask_scores: task
                .subtasks
                .values()
                .map(|st| (st.id, st.max_score))
                .collect(),
            testcase_scores: task
                .subtasks
                .values()
                .map(|st| (st.id, st.testcases.keys().map(|tc| (*tc, None)).collect()))
                .collect(),
            aggregator: task.testcase_score_aggregator.clone(),
        }
    }

    /// Store the score of the testcase and eventually compute the score of the subtask and of the
    /// task.
    pub fn score(
        &mut self,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        score: f64,
        message: String,
        sender: Arc<Mutex<UIMessageSender>>,
        solution: PathBuf,
    ) -> Result<(), Error> {
        self.testcase_scores
            .get_mut(&subtask_id)
            .unwrap()
            .insert(testcase_id, Some(score));
        sender.send(UIMessage::IOITestcaseScore {
            subtask: subtask_id,
            testcase: testcase_id,
            solution: solution.clone(),
            score,
            message,
        })?;
        if self.testcase_scores[&subtask_id]
            .values()
            .all(Option::is_some)
        {
            let subtask_score = self.max_subtask_scores[&subtask_id]
                * self.aggregator.aggregate(
                    self.testcase_scores[&subtask_id]
                        .values()
                        .map(|score| score.unwrap()),
                );
            self.subtask_scores.insert(subtask_id, Some(subtask_score));
            sender.send(UIMessage::IOISubtaskScore {
                subtask: subtask_id,
                solution: solution.clone(),
                score: subtask_score,
            })?;
            if self.subtask_scores.values().all(Option::is_some) {
                let task_score: f64 = self
                    .subtask_scores
                    .values()
                    .map(|score| score.unwrap())
                    .sum();
                sender.send(UIMessage::IOITaskScore {
                    solution: solution.clone(),
                    score: task_score,
                })?;
            }
        }
        Ok(())
    }
}
