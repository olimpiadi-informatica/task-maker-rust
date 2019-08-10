use crate::evaluation::*;
use crate::task_types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// In IOI tasks the subtask numbers are non-negative integers
pub type IOISubtaskId = u32;
/// In IOI tasks the testcase numbers are non-negative integers
pub type IOITestcaseId = u32;

/// Information about a generic IOI task
#[derive(Debug)]
pub struct IOITaskInfo {
    /// Path of the directory of the task.
    pub path: PathBuf,
    /// The information from the yaml file
    pub yaml: IOITaskYAML,
    /// The list of the subtasks
    pub subtasks: HashMap<IOISubtaskId, IOISubtaskInfo>,
    /// The list of the testcases of each subtask
    pub testcases: HashMap<IOISubtaskId, HashMap<IOITestcaseId, IOITestcaseInfo>>,
    /// The checker to use for this task
    pub checker: Box<Checker<IOISubtaskId, IOITestcaseId>>,
    /// The score type to use for this task
    pub score_type: Box<ScoreType<IOISubtaskId, IOITestcaseId>>,
}

/// Deserialized data from the task.yaml of a IOI format task
#[derive(Debug, Serialize, Deserialize)]
pub struct IOITaskYAML {
    /// The name of the task (the short one)
    #[serde(alias = "nome_breve")]
    pub name: String,
    /// The title of the task (the long one)
    #[serde(alias = "nome")]
    pub title: String,
    /// The number of input files, if not provided will be autodetected
    pub n_input: Option<u32>,
    /// The score mode for this task, task-maker will ignore this.
    pub score_mode: Option<String>,
    /// The score type to use for this task.
    pub score_type: Option<String>,
    /// The token mode of this task.
    ///
    /// This is ignored by task-maker.
    pub token_mode: Option<String>,

    /// The timelimit for the execution of the solutions, if not set it's
    /// unlimited
    #[serde(alias = "timeout")]
    pub time_limit: Option<f64>,
    /// The memory limit in MiB of the execution of the solution, if not set
    /// it's unlimited.
    #[serde(alias = "memlimit")]
    pub memory_limit: Option<u64>,
    /// A list of comma separated numbers of the testcases with the feedback,
    /// can be set to "all".
    ///
    /// This is ignored by task-maker.
    #[serde(alias = "risultati")]
    pub public_testcases: Option<String>,
    /// Whether this is an output only task. Defaults to false.
    #[serde(default = "bool::default")]
    pub output_only: bool,
    /// The maximum score of this task, if it's not set it will be
    /// autodetected from the testcase definition.
    pub total_value: Option<f64>,
    /// The input file for the solutions, usually 'input.txt' or '' (stdin)
    pub infile: String,
    /// The output file for the solutions, usually 'output.txt' or '' (stdout).
    pub outfile: String,
    /// The primary language for this task.
    pub primary_language: Option<String>,
}

/// A subtask of a IOI task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IOISubtaskInfo {
    /// The maximum score of the subtask, must be >= 0
    pub max_score: f64,
}

/// A testcase of a IOI task. Every testcase has an input and an output that
/// will be put in the input/ and output/ folders. The files are written there
/// only if it's not a dry-run and if the files are not static.
#[derive(Debug)]
pub struct IOITestcaseInfo {
    /// The number of the testcase
    pub testcase: IOITestcaseId,
    /// The generator of this testcase
    pub generator: Arc<Generator<IOISubtaskId, IOITestcaseId>>,
    /// The validator of this testcase
    pub validator: Option<Arc<Validator<IOISubtaskId, IOITestcaseId>>>,
    /// The official solution of this testcase
    pub solution: Arc<Solution<IOISubtaskId, IOITestcaseId>>,
}

/// A generator formed by a source file and a list of arguments to pass to it.
#[derive(Debug)]
pub struct IOIGenerator {
    /// The source file with the generator.
    pub source_file: Arc<SourceFile>,
    /// The list of arguments to pass to the generator
    pub args: Vec<String>,
}

/// A validator of a testcase.
#[derive(Debug)]
pub struct IOIValidator {
    /// The source file with the validator.
    pub source_file: Arc<SourceFile>,
    /// The list of arguments to pass to the validator.
    pub args: Vec<String>,
}

/// A solution for a task, not necessary the official one.
#[derive(Debug)]
pub struct IOISolution {
    /// The source file with the solution.
    pub source_file: Arc<SourceFile>,
    /// The input file the solution is expecting, None for stdin.
    pub infile: Option<PathBuf>,
    /// The output file the solution is writing to. None for stdout.
    pub outfile: Option<PathBuf>,
    /// Limits to set to the execution of the solution.
    pub limits: ExecutionLimits,
}

/// Evaluation options for a IOI task
pub struct IOIEvaluationOptions;

impl IOIGenerator {
    /// Make a new IOIGenerator based on that source file and those args.
    pub fn new(source_file: Arc<SourceFile>, args: Vec<String>) -> IOIGenerator {
        IOIGenerator { source_file, args }
    }
}

impl IOIValidator {
    /// Make a new IOIValidator based on that source file and those args.
    pub fn new(source_file: Arc<SourceFile>, args: Vec<String>) -> IOIValidator {
        IOIValidator { source_file, args }
    }
}

impl IOISolution {
    /// Make a new IOISolution based on that source file.
    pub fn new(
        source_file: Arc<SourceFile>,
        infile: Option<PathBuf>,
        outfile: Option<PathBuf>,
        limits: ExecutionLimits,
    ) -> IOISolution {
        IOISolution {
            source_file,
            infile,
            outfile,
            limits,
        }
    }
}

impl SubtaskInfo for IOISubtaskInfo {
    fn max_score(&self) -> f64 {
        self.max_score
    }
}

impl TestcaseInfo<IOISubtaskId, IOITestcaseId> for IOITestcaseInfo {
    fn write_input_to(&self) -> Option<PathBuf> {
        Some(Path::new("input").join(format!("input{}.txt", self.testcase)))
    }

    fn write_output_to(&self) -> Option<PathBuf> {
        Some(Path::new("output").join(format!("output{}.txt", self.testcase)))
    }

    fn generator(&self) -> Arc<Generator<IOISubtaskId, IOITestcaseId>> {
        self.generator.clone()
    }

    fn validator(&self) -> Option<Arc<Validator<IOISubtaskId, IOITestcaseId>>> {
        self.validator.clone()
    }

    fn solution(&self) -> Arc<Solution<IOISubtaskId, IOITestcaseId>> {
        self.solution.clone()
    }
}

impl Generator<IOISubtaskId, IOITestcaseId> for IOIGenerator {
    fn generate(
        &self,
        eval: &mut EvaluationData,
        subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> (File, Option<Execution>) {
        let mut exec = self.source_file.execute(
            eval,
            &format!("Generation of testcase {}", testcase),
            self.args.clone(),
        );
        let stdout = exec.stdout();
        eval.sender
            .send(UIMessage::IOIGeneration {
                subtask,
                testcase,
                status: UIExecutionStatus::Pending,
            })
            .unwrap();
        (stdout, Some(exec))
    }
}

impl Validator<IOISubtaskId, IOITestcaseId> for IOIValidator {
    fn validate(
        &self,
        eval: &mut EvaluationData,
        input: File,
        _subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> (File, Option<Execution>) {
        let mut exec = self.source_file.execute(
            eval,
            &format!("Validation of testcase {}", testcase),
            self.args.clone(),
        );
        exec.stdin(&input);
        exec.input(&input, Path::new("input.txt"), false);
        let stdout = exec.stdout();
        (stdout, Some(exec))
    }
}

impl Solution<IOISubtaskId, IOITestcaseId> for IOISolution {
    fn solve(
        &self,
        eval: &mut EvaluationData,
        input: File,
        validation: Option<File>,
        _subtask: IOISubtaskId,
        testcase: IOITestcaseId,
    ) -> (File, Option<Execution>) {
        let mut exec = self.source_file.execute(
            eval,
            &format!(
                "Execution of {} on testcase {}",
                self.source_file.name(),
                testcase
            ),
            vec![],
        );
        if let Some(infile) = &self.infile {
            exec.input(&input, infile, false);
        } else {
            exec.stdin(&input);
        }
        let output = if let Some(outfile) = &self.outfile {
            exec.output(outfile)
        } else {
            exec.stdout()
        };
        if let Some(validation) = validation {
            exec.input(&validation, Path::new("_tm_validation"), false);
        }
        exec.limits = self.limits.clone();
        (output, Some(exec))
    }
}

impl EvaluationOptions for IOIEvaluationOptions {
    fn dry_run(&self) -> bool {
        false
    }
    fn cache_mode(&self) -> bool {
        false
    }
}
