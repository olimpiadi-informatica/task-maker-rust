//! The `italian_yaml` format is defined by [`cms`](https://cms.readthedocs.io/en/v1.4/External%20contest%20formats.html#italian-import-format)
//! and it's the most used format in the Italian Olympiads.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use task_maker_lang::GraderMap;

use crate::ioi::{
    Checker, InputValidator, OutputGenerator, SubtaskId, SubtaskInfo, Task, TaskType, TestcaseId,
    TestcaseInfo, TestcaseScoreAggregator,
};
use crate::{find_source_file, list_files};

mod gen_gen;
mod static_inputs;

/// Deserialized data from the task.yaml of a IOI format task.
#[derive(Debug, Serialize, Deserialize)]
struct TaskYAML {
    /// The name of the task (the short one).
    #[serde(alias = "nome_breve")]
    pub name: String,
    /// The title of the task (the long one).
    #[serde(alias = "nome")]
    pub title: String,
    /// The number of input files.
    ///
    /// This is ignored by task-maker.
    pub n_input: Option<u32>,
    /// The score mode for this task.
    ///
    /// This is ignored by task-maker.
    pub score_mode: Option<String>,
    /// The score type to use for this task.
    pub score_type: Option<String>,
    /// The token mode of this task.
    ///
    /// This is ignored by task-maker.
    pub token_mode: Option<String>,

    /// The time limit for the execution of the solutions, if not set it's unlimited.
    #[serde(alias = "timeout")]
    pub time_limit: Option<f64>,
    /// The memory limit in MiB of the execution of the solution, if not set it's unlimited.
    #[serde(alias = "memlimit")]
    pub memory_limit: Option<u64>,
    /// A list of comma separated numbers of the testcases with the feedback, can be set to "all".
    ///
    /// This is ignored by task-maker.
    #[serde(alias = "risultati")]
    pub public_testcases: Option<String>,
    /// Whether this is an output only task. Defaults to false.
    #[serde(default = "bool::default")]
    #[serde(serialize_with = "python_bool_serializer")]
    #[serde(deserialize_with = "python_bool_deserializer")]
    pub output_only: bool,
    /// The maximum score of this task, if it's not set it will be autodetected from the testcase
    /// definition.
    ///
    /// This is ignored by task-maker.
    pub total_value: Option<f64>,
    /// The input file for the solutions, usually 'input.txt' or '' (stdin). Defaults to `''`.
    #[serde(default = "String::default")]
    pub infile: String,
    /// The output file for the solutions, usually 'output.txt' or '' (stdout). Defaults to `''`.
    #[serde(default = "String::default")]
    pub outfile: String,
    /// The primary language for this task.
    ///
    /// This is ignored by task-maker.
    pub primary_language: Option<String>,
}

/// The iterator item type when following the task input testcases.
#[derive(Debug)]
pub(crate) enum TaskInputEntry {
    /// Create a new subtask given its information.
    Subtask(SubtaskInfo),
    /// Create a new testcase inside the last subtask given its information. `Testcase` can be sent
    /// only after at least one `Subtask`.
    Testcase(TestcaseInfo),
}

/// Given the path to the task directory, try to parse the task inside of it assuming the format is
/// `italian_yaml`.
///
/// `italian_yaml` format is structured as follow:
/// * `task.yaml` - file with the task information
/// * `gen/` - folder with the generator and validator
///     * `generator.xxx` (also `generatore`)
///     * `validator.xxx` (also `valida`)
///     * `GEN` - subtask and testcase specifications
/// * `sol/` - folder with solutions, graders and stubs
///     * `solution.xxx` the official solution (also `soluzione`)
///     * other solutions with different names
/// * `check/` - folder with the checker (also `cor/`)
///     * `checker.xxx` (also `correttore`)
/// * `input/` - folder with the input files
/// * `output/` - folder with the output files
/// * `statement/` - folder with the statement (also `testo`)
///
/// A task must have a generator (and a GEN file) or the input files should be  put in `input/`.
/// The official solution must be present or the output files should be put in `output/`.
pub fn parse_task<P: AsRef<Path>>(task_dir: P) -> Result<Task, Error> {
    let task_dir = task_dir.as_ref();
    let yaml: TaskYAML = serde_yaml::from_reader(fs::File::open(&task_dir.join("task.yaml"))?)?;
    debug!("The yaml is {:#?}", yaml);

    let map_file = |file: String| -> Option<PathBuf> {
        match file.as_ref() {
            "" => None,
            _ => Some(file.into()),
        }
    };
    let infile = map_file(yaml.infile);
    let outfile = map_file(yaml.outfile);

    let graders = list_files(task_dir, vec!["sol/grader.*", "sol/stub.*"]);
    let grader_map = Arc::new(GraderMap::new(graders));
    debug!("The graders are: {:#?}", grader_map);

    let gen_gen = task_dir.join("gen").join("GEN");
    let inputs = if gen_gen.exists() {
        debug!("Parsing testcases from gen/GEN");
        gen_gen::parse_gen_gen(
            &gen_gen,
            detect_validator(task_dir.to_path_buf()),
            detect_output_generator(task_dir.to_path_buf(), grader_map.clone()),
        )?
    } else {
        debug!("Using testcases inside input/");
        static_inputs::static_inputs(
            task_dir,
            detect_validator(task_dir.to_path_buf()),
            detect_output_generator(task_dir.to_path_buf(), grader_map.clone()),
        )
    };

    let mut subtasks = HashMap::new();
    let mut last_subtask: Option<SubtaskInfo> = None;
    for input in inputs {
        match input {
            TaskInputEntry::Subtask(subtask) => {
                if let Some(last_subtask) = last_subtask.take() {
                    subtasks.insert(last_subtask.id, last_subtask);
                }
                last_subtask = Some(subtask);
            }
            TaskInputEntry::Testcase(testcase) => {
                last_subtask
                    .as_mut()
                    .expect("Testcase before Subtask")
                    .testcases
                    .insert(testcase.id, testcase);
            }
        }
    }
    // insert the last subtask to the map
    if let Some(subtask) = last_subtask.take() {
        subtasks.insert(subtask.id, subtask);
    }

    let custom_checker = find_source_file(
        task_dir,
        vec![
            "check/checker.*",
            "cor/correttore.*",
            "check/checker",
            "cor/correttore",
        ],
        None,
        Some(task_dir.join("check").join("checker")),
    )
    .map(Arc::new)
    .map(Checker::Custom);

    Ok(Task {
        path: task_dir.into(),
        task_type: TaskType::Batch,
        name: yaml.name,
        title: yaml.title,
        time_limit: yaml.time_limit,
        memory_limit: yaml.memory_limit,
        infile,
        outfile,
        subtasks,
        checker: custom_checker.unwrap_or(Checker::WhiteDiff),
        testcase_score_aggregator: yaml
            .score_type
            .as_ref()
            .map(|s| TestcaseScoreAggregator::from_str(s))
            .unwrap_or(Ok(TestcaseScoreAggregator::Min))?,
        grader_map,
    })
}

/// Search for a valid input validator inside the task directory. Will return a function that, given
/// a subtask id, returns an `InputValidator` using that validator. If no validator is found,
/// `InputValidator::AssumeValid` is used.
fn detect_validator(task_dir: PathBuf) -> impl Fn(SubtaskId) -> InputValidator {
    let validator = find_source_file(
        &task_dir,
        vec![
            "gen/validator.*",
            "gen/valida.*",
            "gen/validator",
            "gen/valida",
        ],
        None,
        Some(task_dir.join("bin").join("validator")),
    )
    .map(Arc::new);
    debug!("Detected input validator: {:?}", validator);
    move |st: SubtaskId| -> InputValidator {
        if let Some(validator) = validator.as_ref() {
            InputValidator::Custom(
                validator.clone(),
                vec!["tm_validation_file".to_string(), st.to_string()],
            )
        } else {
            InputValidator::AssumeValid
        }
    }
}

/// Search for a valid output generator (aka official solution) inside the task directory. Will
/// return a function that, given a testcase id, returns an `OutputGenerator` using that generator.
/// If no generator is found, `OutputGenerator::StaticFile` is used instead.
fn detect_output_generator(
    task_dir: PathBuf,
    grader_map: Arc<GraderMap>,
) -> impl Fn(TestcaseId) -> OutputGenerator {
    let official_solution = find_source_file(
        &task_dir,
        vec![
            "sol/solution.*",
            "sol/soluzione.*",
            "sol/solution",
            "sol/soluzione",
        ],
        Some(grader_map.clone()),
        Some(task_dir.join("bin").join("official_solution")),
    )
    .map(Arc::new);
    debug!("Detected output generator: {:?}", official_solution);
    let output_directory = task_dir.join("output");
    move |tc: TestcaseId| -> OutputGenerator {
        if let Some(solution) = official_solution.as_ref() {
            OutputGenerator::Custom(solution.clone(), vec![])
        } else {
            OutputGenerator::StaticFile(output_directory.join(format!("output{}.txt", tc)))
        }
    }
}

/// Serializer of a boolean using the python syntax:
/// - `true` -> `True`
/// - `false` -> `False`
#[allow(clippy::trivially_copy_pass_by_ref)]
fn python_bool_serializer<S>(val: &bool, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *val {
        ser.serialize_str("True")
    } else {
        ser.serialize_str("False")
    }
}

/// Deserializer of a boolean using the python syntax:
/// - `True` -> `true`
/// - `False` -> `false`
/// - other -> error
fn python_bool_deserializer<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let val = String::deserialize(deser)?;
    if val == "True" {
        Ok(true)
    } else if val == "False" {
        Ok(false)
    } else {
        Err(Error::custom("invalid bool, either True or False"))
    }
}
