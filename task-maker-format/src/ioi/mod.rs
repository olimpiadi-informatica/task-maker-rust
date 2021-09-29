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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Error};
use serde::{Deserialize, Serialize};
use typescript_definitions::TypeScriptify;

use curses_ui::CursesUI;
pub use dag::*;
pub use format::italian_yaml;
pub use statement::*;
pub use task_info::*;
use task_maker_lang::GraderMap;
pub use ui_state::*;

use crate::ioi::format::italian_yaml::TM_ALLOW_DELETE_COOKIE;
use crate::ioi::italian_yaml::is_gen_gen_deletable;
use crate::sanity_checks::SanityChecks;
use crate::ui::*;
use crate::{EvaluationConfig, EvaluationData, TaskFormat, TaskInfo, UISender};

mod curses_ui;
mod dag;
pub(crate) mod finish_ui;
mod format;
pub mod sanity_checks;
mod statement;
pub(crate) mod task_info;
pub(crate) mod ui_state;

/// In IOI tasks the subtask numbers are non-negative 0-based integers.
pub type SubtaskId = u32;
/// In IOI tasks the testcase numbers are non-negative 0-based integers.
pub type TestcaseId = u32;

/// This struct will manage the scores of a solution in a task and will emit the ui messages when
/// a new score is ready.
#[derive(Debug, Clone)]
pub struct ScoreManager {
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
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct IOITask {
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
    /// The default input validator for this task, if any.
    #[serde(skip_serializing)]
    pub input_validator: InputValidator,
    /// The aggregator to use to compute the score of the subtask based on the score of the
    /// testcases.
    pub testcase_score_aggregator: TestcaseScoreAggregator,
    /// The graders registered for this task.
    pub grader_map: Arc<GraderMap>,
    /// The booklets to compile for this task.
    pub booklets: Vec<Booklet>,
    /// An integer that defines the difficulty of the task. Used only in booklet compilations.
    pub difficulty: Option<u8>,
    /// An integer that defines the level inside a _syllabus_ (for example for the Olympiads in
    /// Teams). Used only in booklet compilations.
    pub syllabus_level: Option<u8>,
    /// The sanity checks attached to this task. Wrapped in Arc since `SanityChecks` is not Clone.
    /// It's also not `Serialize` nor `Deserialize`, all the sanity checks will be lost on
    /// serialization.
    #[serde(skip_serializing, skip_deserializing)]
    pub sanity_checks: Arc<SanityChecks<IOITask>>,
}

/// A subtask of a IOI task.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct SubtaskInfo {
    /// The id of the subtask.
    pub id: SubtaskId,
    /// Textual description of the subtask.
    pub description: Option<String>,
    /// The maximum score of the subtask, must be >= 0.
    pub max_score: f64,
    /// The testcases inside this subtask.
    pub testcases: HashMap<TestcaseId, TestcaseInfo>,
}

/// A testcase of a IOI task.
///
/// Every testcase has an input and an output that will be put in the input/ and output/ folders.
/// The files are written there only if it's not a dry-run and if the files are not static.
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
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

impl IOITask {
    /// Try to make a `Task` from the specified path. Will return `Err` if the format of the task
    /// is not IOI or if the task is corrupted and cannot be parsed.
    pub fn new<P: AsRef<Path>>(path: P, eval_config: &EvaluationConfig) -> Result<IOITask, Error> {
        format::italian_yaml::parse_task(path, eval_config)
    }

    /// Check if in the provided path there could be a IOI-like task.
    pub fn is_valid<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();
        path.join("task.yaml").exists()
            && (path.join("gen/GEN").exists()
                || path.join("gen/cases.gen").exists()
                || path.join("input").is_dir())
    }
}

impl TaskFormat for IOITask {
    fn path(&self) -> &Path {
        &self.path
    }

    fn ui(&self, ui_type: &UIType) -> Result<Box<dyn UI>, Error> {
        match ui_type {
            UIType::Raw => Ok(Box::new(RawUI::new())),
            UIType::Print => Ok(Box::new(PrintUI::new())),
            UIType::Curses => Ok(Box::new(
                CursesUI::new(UIState::new(self)).context("Cannot build curses UI")?,
            )),
            UIType::Json => Ok(Box::new(JsonUI::new())),
            UIType::Silent => Ok(Box::new(SilentUI::new())),
        }
    }

    fn build_dag(&self, eval: &mut EvaluationData, config: &EvaluationConfig) -> Result<(), Error> {
        eval.sender.send(UIMessage::IOITask {
            task: Box::new(self.clone()),
        })?;
        self.sanity_checks
            .pre_hook(self, eval)
            .context("Sanity check pre-hooks failed")?;
        let empty_score_manager = ScoreManager::new(self);
        let solutions: Vec<_> = config
            .filter_solutions(&self.path, vec!["sol/*"], Some(self.grader_map.clone()))
            .into_iter()
            .map(|source| (source, Arc::new(Mutex::new(empty_score_manager.clone()))))
            .collect();

        self.task_type
            .prepare_dag(eval)
            .context("Failed to prepare DAG")?;

        for subtask in self.subtasks.values() {
            trace!("Executing the generation of subtask {}", subtask.id);

            for testcase in subtask.testcases.values() {
                trace!(
                    "Executing the generation of testcase {} of subtask {}",
                    testcase.id,
                    subtask.id
                );

                let input = testcase
                    .input_generator
                    .generate_and_bind(eval, subtask.id, testcase.id)
                    .context("Failed to bind input generator")?;
                let val_handle = testcase
                    .input_validator
                    .validate_and_bind(eval, subtask.id, testcase.id, input)
                    .context("Failed to bind validator")?;
                let output = testcase
                    .output_generator
                    .generate_and_bind(self, eval, subtask.id, testcase.id, input, val_handle)
                    .context("Failed to bind output generator")?;

                for (solution, score_manager) in solutions.iter() {
                    trace!(
                        "Evaluation of the solution {:?} against subtask {} / testcase {}",
                        solution.name(),
                        subtask.id,
                        testcase.id
                    );

                    self.task_type
                        .evaluate(
                            self,
                            eval,
                            subtask.id,
                            testcase.id,
                            solution,
                            input,
                            val_handle,
                            output,
                            score_manager.clone(),
                        )
                        .context("Failed to bind evaluation")?;
                }
            }
        }
        for booklet in self.booklets.iter() {
            booklet
                .build(eval)
                .context("Failed to bind booklet compilation")?;
        }
        Ok(())
    }

    fn sanity_check_post_hook(&self, ui: &mut UIMessageSender) -> Result<(), Error> {
        self.sanity_checks.post_hook(self, ui)
    }

    fn clean(&self) -> Result<(), Error> {
        for dir in &["input", "output"] {
            let dir = self.path.join(dir);
            if !dir.exists() {
                continue;
            }
            for file in glob::glob(dir.join("*.txt").to_string_lossy().as_ref())
                .context("Invalid glob pattern")?
            {
                let file = match file {
                    Ok(file) => file,
                    Err(e) => {
                        warn!("Glob error: {:?}", e);
                        continue;
                    }
                };
                // check if the file is used by a static generator
                if self
                    .subtasks
                    .values()
                    .flat_map(|st| st.testcases.values())
                    .any(|tc| match (&tc.input_generator, &tc.output_generator) {
                        (InputGenerator::StaticFile(path), _)
                        | (_, OutputGenerator::StaticFile(path)) => path == &file,
                        _ => false,
                    })
                {
                    continue;
                }
                info!("Removing {}", file.display());
                std::fs::remove_file(&file)
                    .with_context(|| format!("Failed to clean file {}", file.display()))?;
            }
            info!("Removing {}", dir.display());
            if let Err(e) = std::fs::remove_dir(&dir) {
                // FIXME: this should be `e.kind() == ErrorKind::DirectoryNotEmpty`, but it is not
                //        stable yet.
                if e.to_string().contains("Directory not empty") {
                    warn!("Directory {} not empty!", dir.display());
                } else {
                    Err(e).with_context(|| format!("Cannot remove {}", dir.display()))?;
                }
            }
        }
        // remove the bin/ folder
        let bin_path = self.path.join("bin");
        if bin_path.exists() {
            info!("Removing {}", bin_path.display());
            std::fs::remove_dir_all(&bin_path).with_context(|| {
                format!("Failed to remove bin/ directory at {}", bin_path.display())
            })?;
        }
        // remove the compiled checkers
        if let TaskType::Batch(data) = &self.task_type {
            if let Checker::Custom(_) = data.checker {
                for checker in &["check/checker", "cor/correttore"] {
                    let path = self.path.join(checker);
                    if path.exists() {
                        info!("Removing {}", path.display());
                        std::fs::remove_file(&path).with_context(|| {
                            format!("Failed to remove compiled checker at {}", path.display())
                        })?;
                    }
                }
            }
        }
        // remove the gen/GEN if there is cases.gen
        let gen_gen_path = self.path.join("gen/GEN");
        let cases_gen_path = self.path.join("gen/cases.gen");
        if cases_gen_path.exists() && gen_gen_path.exists() {
            if is_gen_gen_deletable(&gen_gen_path)? {
                info!("Removing {}", gen_gen_path.display());
                std::fs::remove_file(&gen_gen_path).with_context(|| {
                    format!("Failed to remove gen/GEN at {}", gen_gen_path.display())
                })?;
            } else {
                warn!(
                    "Won't remove gen/GEN since it doesn't contain {}",
                    TM_ALLOW_DELETE_COOKIE
                );
            }
        }
        Ok(())
    }

    fn task_info(&self) -> Result<TaskInfo, Error> {
        Ok(TaskInfo::IOI(
            task_info::IOITaskInfo::new(self).context("Cannot produce IOI task info")?,
        ))
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
    pub fn new(task: &IOITask) -> ScoreManager {
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
            .ok_or_else(|| anyhow!("Unknown subtask {}", subtask_id))?
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
            let normalized_score = self.aggregator.aggregate(
                self.testcase_scores[&subtask_id]
                    .values()
                    .map(|score| score.unwrap()),
            );
            let subtask_score = self.max_subtask_scores[&subtask_id] * normalized_score;
            self.subtask_scores.insert(subtask_id, Some(subtask_score));
            sender.send(UIMessage::IOISubtaskScore {
                subtask: subtask_id,
                solution: solution.clone(),
                score: subtask_score,
                normalized_score,
            })?;
            if self.subtask_scores.values().all(Option::is_some) {
                let task_score: f64 = self
                    .subtask_scores
                    .values()
                    .map(|score| score.unwrap())
                    .sum();
                sender.send(UIMessage::IOITaskScore {
                    solution,
                    score: task_score,
                })?;
            }
        }
        Ok(())
    }
}
