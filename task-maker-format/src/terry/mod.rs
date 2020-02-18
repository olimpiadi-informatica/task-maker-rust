//! The Terry task format.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use failure::{bail, Error};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::terry::dag::{Checker, InputGenerator, InputValidator, Solution};
use crate::terry::format::parse_task;
use crate::ui::{JsonUI, PrintUI, RawUI, SilentUI, UIMessage, UIMessageSender, UIType, UI};
use crate::{EvaluationConfig, EvaluationData, SourceFile, TaskFormat, TaskInfo, UISender};

mod dag;
pub(crate) mod finish_ui;
mod format;
pub(crate) mod ui_state;

/// The type of the seed of a generator for an input file.
pub type Seed = u64;

/// Information about a generic Terry task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Path of the directory of the task.
    pub path: PathBuf,
    /// The name of the task (the short one).
    pub name: String,
    /// The title of the task (the long one).
    pub description: String,
    /// The maximum score for this task.
    pub max_score: f64,

    /// The generator of input files of this task.
    #[serde(skip_serializing)]
    pub generator: InputGenerator,
    /// The validator of input files of this task.
    #[serde(skip_serializing)]
    pub validator: Option<InputValidator>,
    /// The checker of input/output files of this task.
    #[serde(skip_serializing)]
    pub checker: Checker,
    /// The official solution of this task, if any. Will be compiled and placed in the sandbox of
    /// the generation/validation/checking.
    #[serde(skip_serializing)]
    pub official_solution: Option<Arc<SourceFile>>,
}

/// The output of the checker for a solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionOutcome {
    /// The score normalized from 0.0 to 1.0.
    pub score: f64,
    /// The validation outcome of the solution.
    pub validation: SolutionValidation,
    /// The feedback outcome of the solution.
    pub feedback: SolutionFeedback,
}

/// The validation part of the outcome of a solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionValidation {
    /// The validation of the test cases, in the same order as the input.
    pub cases: Vec<SolutionValidationCase>,
    /// The alerts sent by the checker regarding the validation.
    pub alerts: Vec<SolutionAlert>,
}

/// The validation outcome of a test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionValidationCase {
    /// The status of the testcase.
    pub status: CaseStatus,
    /// An optional message associated to the test case.
    pub message: Option<String>,
}

/// The possible statuses of the validation of a test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaseStatus {
    /// The testcase is not present in the output file.
    Missing,
    /// The testcase is present and correctly parsed.
    Parsed,
    /// The testcase is present but its format is invalid.
    Invalid,
}

/// The feedback part of the outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionFeedback {
    /// The feedback of each testcase, in the same order as the input.
    pub cases: Vec<SolutionFeedbackCase>,
    /// The alerts sent by the checker regarding the feedback.
    pub alerts: Vec<SolutionAlert>,
}

/// The feedback of a test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionFeedbackCase {
    /// Whether this testcase is correct.
    pub correct: bool,
    /// An optional message associated to the test case.
    pub message: Option<String>,
}

/// A message with an associated severity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionAlert {
    /// The severity of the alert message.
    pub severity: String,
    /// The content of the alert.
    pub message: String,
}

impl Task {
    /// Try to make a `Task` from the specified path. Will return `Err` if the format of the task
    /// is not Terry or if the task is corrupted and cannot be parsed.
    pub fn new<P: AsRef<Path>>(path: P, eval_config: &EvaluationConfig) -> Result<Task, Error> {
        parse_task(path.as_ref(), eval_config)
    }

    /// Check if in the provided path there could be a Terry-like task.
    pub fn is_valid<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().join("task.yaml").exists() && path.as_ref().join("managers").is_dir()
    }
}

impl TaskFormat for Task {
    fn path(&self) -> &Path {
        &self.path
    }

    fn ui(&self, ui_type: &UIType) -> Result<Box<dyn UI>, Error> {
        match ui_type {
            UIType::Raw => Ok(Box::new(RawUI::new())),
            UIType::Json => Ok(Box::new(JsonUI::new())),
            UIType::Silent => Ok(Box::new(SilentUI::new())),
            UIType::Print => Ok(Box::new(PrintUI::new())),
            _ => bail!("Not yet supported UI: {:?}", ui_type),
        }
    }

    fn build_dag(&self, eval: &mut EvaluationData, config: &EvaluationConfig) -> Result<(), Error> {
        eval.sender.send(UIMessage::TerryTask {
            task: Box::new(self.clone()),
        })?;
        // TODO: call pre hook
        let solutions = config.filter_solutions(&self.path, vec!["solutions/*"], None);
        let mut rng = rand::thread_rng();
        for solution in solutions {
            let seed = if let Some(seed) = config.seed {
                seed
            } else {
                rng.gen_range(0, 1 << 31)
            };
            let input_file = self.generator.generate_and_bind(
                eval,
                &solution,
                seed,
                self.official_solution.clone(),
            )?;
            let validation_file = if let Some(validator) = self.validator.as_ref() {
                Some(validator.validate_and_bind(
                    eval,
                    &solution,
                    input_file,
                    self.official_solution.clone(),
                )?)
            } else {
                None
            };
            let output_file =
                Solution::solve_and_bind(eval, &solution, input_file, validation_file)?;
            let sender = eval.sender.clone();
            let solution_path = solution.path.clone();
            self.checker.check_and_bind(
                eval,
                &solution,
                input_file,
                output_file,
                self.official_solution.clone(),
                move |outcome| {
                    sender.send(UIMessage::TerrySolutionOutcome {
                        solution: solution_path,
                        outcome: outcome.map_err(|e| format!("Invalid checker outcome: {}", e)),
                    })
                },
            )?;
        }
        Ok(())
    }

    fn sanity_check_post_hook(&self, _ui: &mut UIMessageSender) -> Result<(), Error> {
        // TODO implement the sanity checks post hook
        Ok(())
    }

    fn clean(&self) -> Result<(), Error> {
        bail!("Cleaning a terry task is not supported yet");
    }

    fn task_info(&self) -> Result<TaskInfo, Error> {
        bail!("Terry task info is not supported yet");
    }
}
