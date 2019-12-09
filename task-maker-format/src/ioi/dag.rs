use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use failure::{bail, Error};
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, ExecutionCommand, ExecutionStatus, File, FileUuid};

use crate::ioi::*;
use crate::ui::UIMessage;
use crate::{EvaluationData, SourceFile, UISender};

const STDERR_CONTENT_LENGTH: usize = 10 * 1024;

/// Which tool to use to compute the score on a testcase given the input file, the _correct_ output
/// file and the output file to evaluate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Checker {
    /// Use a built-in white diff checker that scores 1.0 if the two output files are identical
    /// except for white spaces. It internally uses `diff --ignore-all-spaces`
    WhiteDiff,
    /// Use a custom checker based on an executable that can output a score (from 0.0 to 1.0) to
    /// stdout as well as a custom message on stderr.
    ///
    /// The arguments are the paths of (input, correct_output, test_output). The checker should
    /// output to stdout the score and to stderr a message for the user.
    Custom(Arc<SourceFile>),
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
    /// non-zero return code if and only if the input is invalid.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// The source of the output files. It can either be a statically provided output file or a custom
/// command that will generate an output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputGenerator {
    /// Use the static file as output. The file will be copied without transformations.
    StaticFile(PathBuf),
    /// Use a custom command to generate the output file. The task specification for input/output
    /// files are used.
    Custom(Arc<SourceFile>, Vec<String>),
}

/// The aggregator of testcase scores for computing the subtask score.
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
    /// (called `grader.LANG`). The output is checked with an external program.
    Batch,
}

/// Bind the start/done/skip callbacks of an execution to a ui message sender which sends to the UI
/// messages with the correct status field.
///
/// It's also sent to the UI the message with status `UIExecutionStatus::Pending`.
///
/// It works by first cloning the `extra` arguments for each callback. This is required because each
/// callback has to move inside the needed data. For the same reason also the `UIMessageSender` is
/// cloned and then moved inside the callback. The callbacks then simply send to the UI the value
/// returned by the `enum` lambda.
///
/// # Parameters
/// - `eval: EvaluationData`
/// - `exec_uuid: ExecutionUuid`
/// - `enum` is a lambda that takes one or more arguments:
///   - the first is a `UIExecutionStatus`
///   - the followings are clones of the `extra` parameter
/// - `extra` is a series of identifiers of `Clone`able variables.
#[macro_export]
macro_rules! bind_exec_callbacks {
    ($eval:expr, $exec_uuid:expr, $enum:expr $(,$extra:ident)*) => {
        {
            #[allow(clippy::redundant_closure_call)]
            {
                use crate::UISender;
                use crate::ui::UIExecutionStatus;
                {
                    $(let $extra = $extra.clone();)*
                    let status = UIExecutionStatus::Pending;
                    $eval
                        .sender
                        .send(($enum)(status, $($extra,)*))?;
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_start(&$exec_uuid, move |worker| {
                        let status = UIExecutionStatus::Started { worker };
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_done(&$exec_uuid, move |result| {
                        let status = UIExecutionStatus::Done { result };
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
                {
                    $(let $extra = $extra.clone();)*
                    let sender = $eval.sender.clone();
                    $eval.dag.on_execution_skip(&$exec_uuid, move || {
                        let status = UIExecutionStatus::Skipped;
                        sender.send(($enum)(status, $($extra,)*))
                    });
                }
            }
            Result::<(), Error>::Ok(())
        }
    };
}

/// Bind the input/output of an execution to the input and output file of a testcase. It correctly
/// chooses if using stdin/stdout or using normal files by looking at the value set in the `Task`.
///
/// # Parameters
/// - `exec: Execution`
/// - `task: Task`
/// - `input: File`
/// - `validation_handle: Option<File>`
macro_rules! bind_exec_io {
    ($exec:expr, $task:expr, $input:expr, $validation_handle:expr) => {{
        match &$task.infile {
            None => $exec.stdin($input),
            Some(infile) => $exec.input($input, infile, false),
        };
        match $validation_handle {
            None => {}
            Some(file) => {
                $exec.input(file, "wait_for_validation", false);
            }
        };
        match &$task.outfile {
            None => $exec.stdout(),
            Some(outfile) => $exec.output(outfile),
        }
    }};
}

impl InputGenerator {
    /// Add the generation of the input file to the DAG and the callbacks to the UI, returning the
    /// handle to the input file.
    pub(crate) fn generate(
        &self,
        task: &Task,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
    ) -> Result<FileUuid, Error> {
        match self {
            InputGenerator::StaticFile(path) => {
                if !path.exists() {
                    bail!("COPY from not existing file: {:?}", path);
                }
                let file = File::new(format!(
                    "Static input file of testcase {}, subtask {} from {:?}",
                    subtask_id, testcase_id, path
                ));
                let uuid = file.uuid;
                eval.dag.write_file_to(
                    &file,
                    task.path
                        .join("input")
                        .join(format!("input{}.txt", testcase_id)),
                    false,
                );
                eval.dag.provide_file(file, &path)?;
                Ok(uuid)
            }
            InputGenerator::Custom(source_file, args) => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Generation of input file of testcase {}, subtask {}",
                        testcase_id, subtask_id
                    ),
                    args.clone(),
                )?;
                exec.tag(Tag::Generation.into());
                let stdout = exec.stdout();
                bind_exec_callbacks!(eval, exec.uuid, |status| UIMessage::IOIGeneration {
                    subtask: subtask_id,
                    testcase: testcase_id,
                    status
                })?;
                let sender = eval.sender.clone();
                eval.dag
                    .get_file_content(exec.stderr(), STDERR_CONTENT_LENGTH, move |content| {
                        let content = String::from_utf8_lossy(&content);
                        sender.send(UIMessage::IOIGenerationStderr {
                            testcase: testcase_id,
                            subtask: subtask_id,
                            content: content.into(),
                        })
                    });
                eval.dag.add_execution(exec);
                eval.dag.write_file_to(
                    &stdout,
                    task.path
                        .join("input")
                        .join(format!("input{}.txt", testcase_id)),
                    false,
                );
                Ok(stdout.uuid)
            }
        }
    }
}

impl InputValidator {
    /// Add the validation of the input file to the DAG and the callbacks to the UI, optionally
    /// returning a fake file that blocks the usage of the actual input until the validation
    /// succeeds. If the validation is ignored, `None` is returned.
    pub(crate) fn validate(
        &self,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
    ) -> Result<Option<FileUuid>, Error> {
        match self {
            InputValidator::AssumeValid => Ok(None),
            InputValidator::Custom(source_file, args) => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Validation of input file of testcase {}, subtask {}",
                        testcase_id, subtask_id
                    ),
                    args.clone(),
                )?;
                exec.input(input, "tm_validation_file", false)
                    .tag(Tag::Generation.into())
                    .env("TM_SUBTASK", subtask_id.to_string())
                    .env("TM_TESTCASE", testcase_id.to_string());
                let stdout = exec.stdout();
                bind_exec_callbacks!(eval, exec.uuid, |status| UIMessage::IOIValidation {
                    subtask: subtask_id,
                    testcase: testcase_id,
                    status
                })?;
                let sender = eval.sender.clone();
                eval.dag
                    .get_file_content(exec.stderr(), STDERR_CONTENT_LENGTH, move |content| {
                        let content = String::from_utf8_lossy(&content);
                        sender.send(UIMessage::IOIValidationStderr {
                            testcase: testcase_id,
                            subtask: subtask_id,
                            content: content.into(),
                        })
                    });
                eval.dag.add_execution(exec);
                Ok(Some(stdout.uuid))
            }
        }
    }
}

impl OutputGenerator {
    /// Add the generation of the output file to the DAG and the callbacks to the UI, returning the
    /// handle to the output file.
    pub(crate) fn generate(
        &self,
        task: &Task,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<FileUuid, Error> {
        match self {
            OutputGenerator::StaticFile(path) => {
                if !path.exists() {
                    bail!("Static output file not found: {:?}", path);
                }
                let file = File::new(format!(
                    "Static output file of testcase {}, subtask {} from {:?}",
                    subtask_id, testcase_id, path
                ));
                let uuid = file.uuid;
                eval.dag.write_file_to(
                    &file,
                    task.path
                        .join("output")
                        .join(format!("output{}.txt", testcase_id)),
                    false,
                );
                eval.dag.provide_file(file, &path)?;
                Ok(uuid)
            }
            OutputGenerator::Custom(source_file, args) => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Generation of output file of testcase {}, subtask {}",
                        testcase_id, subtask_id
                    ),
                    args.clone(),
                )?;
                exec.tag(Tag::Generation.into());
                let output = bind_exec_io!(exec, task, input, validation_handle);
                bind_exec_callbacks!(eval, exec.uuid, |status| UIMessage::IOISolution {
                    subtask: subtask_id,
                    testcase: testcase_id,
                    status
                })?;
                eval.dag.add_execution(exec);
                eval.dag.write_file_to(
                    &output,
                    task.path
                        .join("output")
                        .join(format!("output{}.txt", testcase_id)),
                    false,
                );
                Ok(output.uuid)
            }
        }
    }
}

impl Checker {
    /// Add the checking of the output file to the DAG, binding the callbacks for sending to the UI
    /// the messages as well as calling `callback` with the outcome of the checker.
    pub(crate) fn check<S: Into<PathBuf>, F>(
        &self,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        solution: S,
        input: FileUuid,
        correct_output: FileUuid,
        test_output: FileUuid,
        callback: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(f64, String) -> Result<(), Error> + Send + Sync + 'static,
    {
        let solution = solution.into();
        match self {
            Checker::WhiteDiff => {
                let mut exec = Execution::new(
                    format!(
                        "Checking output of {:?} of testcase {}, subtask {}",
                        solution.file_name().unwrap(),
                        testcase_id,
                        subtask_id
                    ),
                    ExecutionCommand::system("diff"),
                );
                exec.args(vec!["--ignore-all-space", "correct", "test"])
                    .input(correct_output, "correct", false)
                    .input(test_output, "test", false)
                    .tag(Tag::Checking.into());
                bind_exec_callbacks!(
                    eval,
                    exec.uuid,
                    |status, solution| UIMessage::IOIChecker {
                        subtask: subtask_id,
                        testcase: testcase_id,
                        solution,
                        status
                    },
                    solution
                )?;
                eval.dag.on_execution_done(&exec.uuid, move |result| {
                    match result.status {
                        // diff exits with 0 if the files are equal
                        ExecutionStatus::Success => callback(1.0, "Output is correct".into())?,
                        // return code 1 means the files are different
                        ExecutionStatus::ReturnCode(1) => {
                            callback(0.0, "Output is incorrect".into())?
                        }
                        _ => unreachable!("diff died badly? {:?}", result),
                    };
                    Ok(())
                });
                eval.dag.add_execution(exec);
            }
            Checker::Custom(source_file) => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Checking output of {:?} of testcase {}, subtask {}",
                        solution.file_name().unwrap(),
                        testcase_id,
                        subtask_id
                    ),
                    vec!["input", "correct_output", "test_output"],
                )?;
                exec.input(input, "input", false)
                    .input(correct_output, "correct_output", false)
                    .input(test_output, "test_output", false)
                    .tag(Tag::Checking.into());
                bind_exec_callbacks!(
                    eval,
                    exec.uuid,
                    |status, solution| UIMessage::IOIChecker {
                        subtask: subtask_id,
                        testcase: testcase_id,
                        solution,
                        status
                    },
                    solution
                )?;
                let stdout = exec.stdout();
                let stderr = exec.stderr();
                eval.dag.add_execution(exec);
                // wait for both the stdout and the stderr
                let state_stdout: Arc<Mutex<(Option<f64>, Option<String>)>> =
                    Arc::new(Mutex::new((None, None)));
                let state_stderr = state_stdout.clone();
                let callback_stdout = Arc::new(Mutex::new(Some(callback)));
                let callback_stderr = callback_stdout.clone();
                macro_rules! send_state {
                    ($callback:expr, $state:expr) => {{
                        // if both the score and the message are present
                        if let (Some(ref score), Some(ref message)) = *$state {
                            if let Some(f) = $callback.lock().unwrap().take() {
                                f(*score, message.clone())?;
                            }
                        }
                    }};
                }
                eval.dag.get_file_content(stdout, 128, move |content| {
                    let score = String::from_utf8_lossy(&content);
                    let score: f64 = score
                        .trim()
                        .parse()
                        .map_err(|e| format_err!("Invalid score from checker: {:?}", e))?;
                    let mut state = state_stdout.lock().unwrap();
                    state.0 = Some(score);
                    send_state!(callback_stdout, state);
                    Ok(())
                });
                eval.dag.get_file_content(stderr, 1024, move |content| {
                    let mut state = state_stderr.lock().unwrap();
                    state.1 = Some(String::from_utf8_lossy(&content).trim().to_string());
                    send_state!(callback_stderr, state);
                    Ok(())
                });
            }
        }
        Ok(())
    }
}

impl TaskType {
    /// Evaluate a solution on a testcase, eventually adding to the `ScoreManager` the result of the
    /// evaluation. This will add both the execution as well as the checking to the DAG.
    pub(crate) fn evaluate(
        &self,
        task: &Task,
        eval: &mut EvaluationData,
        subtask_id: SubtaskId,
        testcase_id: TestcaseId,
        source_file: &SourceFile,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
        correct_output: FileUuid,
        score_manager: Arc<Mutex<ScoreManager>>,
    ) -> Result<(), Error> {
        match self {
            TaskType::Batch => {
                let mut exec = source_file.execute(
                    eval,
                    format!(
                        "Evaluation of {} on testcase {}, subtask {}",
                        source_file.name(),
                        testcase_id,
                        subtask_id
                    ),
                    Vec::<String>::new(),
                )?;
                exec.tag(Tag::Evaluation.into());
                let output = bind_exec_io!(exec, task, input, validation_handle);
                let path = source_file.path.clone();
                let limits = exec.limits_mut();
                if let Some(time_limit) = task.time_limit {
                    limits.cpu_time(time_limit);
                    limits.wall_time(time_limit * 1.5 + 1.0); // some margin
                }
                if let Some(memory_limit) = task.memory_limit {
                    limits.memory(memory_limit * 1024); // MiB -> KiB
                }
                bind_exec_callbacks!(
                    eval,
                    exec.uuid,
                    |status, solution| UIMessage::IOIEvaluation {
                        subtask: subtask_id,
                        testcase: testcase_id,
                        solution,
                        status
                    },
                    path
                )?;
                let sender = eval.sender.clone();
                let path = source_file.path.clone();
                let score_manager_err = score_manager.clone();
                eval.dag
                    .on_execution_done(&exec.uuid, move |result| match result.status {
                        ExecutionStatus::Success => Ok(()),
                        _ => score_manager_err.lock().unwrap().score(
                            subtask_id,
                            testcase_id,
                            0.0,
                            format!("{:?}", result.status),
                            sender,
                            path,
                        ),
                    });
                eval.dag.add_execution(exec);

                let sender = eval.sender.clone();
                let path = source_file.path.clone();
                task.checker.check(
                    eval,
                    subtask_id,
                    testcase_id,
                    source_file.path.clone(),
                    input,
                    correct_output,
                    output.uuid,
                    move |score, message| {
                        score_manager.lock().unwrap().score(
                            subtask_id,
                            testcase_id,
                            score,
                            message,
                            sender,
                            path,
                        )
                    },
                )?;
            }
        };
        Ok(())
    }
}

impl TestcaseScoreAggregator {
    /// Aggregate the scores of a subtask from an iterator with the scores of the testcases.
    pub(crate) fn aggregate<I: IntoIterator<Item = f64>>(&self, iter: I) -> f64 {
        match self {
            TestcaseScoreAggregator::Min => iter
                .into_iter()
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(1.0),
            TestcaseScoreAggregator::Sum => {
                let sum_count = iter
                    .into_iter()
                    .fold((0.0, 0), |prev, cur| (prev.0 + cur, prev.1 + 1));
                if sum_count.1 == 0 {
                    return 1.0;
                }
                sum_count.0 / (f64::from(sum_count.1))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult};
    use task_maker_lang::GraderMap;

    fn make_task<P: Into<PathBuf>>(path: P) -> Task {
        Task {
            path: path.into(),
            task_type: TaskType::Batch,
            name: "".to_string(),
            title: "".to_string(),
            time_limit: None,
            memory_limit: None,
            infile: None,
            outfile: None,
            subtasks: Default::default(),
            checker: Checker::WhiteDiff,
            testcase_score_aggregator: TestcaseScoreAggregator::Min,
            grader_map: Arc::new(GraderMap::new(Vec::<PathBuf>::new())),
            booklets: vec![],
            difficulty: None,
            syllabus_level: None,
        }
    }

    #[test]
    fn test_aggregate_min() {
        let aggregator = TestcaseScoreAggregator::Min;
        let min = aggregator.aggregate(vec![1.0, 0.1, 0.5]);
        assert_abs_diff_eq!(0.1, min);
    }

    #[test]
    fn test_aggregate_min_empty() {
        let aggregator = TestcaseScoreAggregator::Min;
        let min = aggregator.aggregate(vec![]);
        assert_abs_diff_eq!(1.0, min);
    }

    #[test]
    fn test_aggregate_sum() {
        let aggregator = TestcaseScoreAggregator::Sum;
        let sum = aggregator.aggregate(vec![1.0, 0.1, 0.7]);
        assert_abs_diff_eq!(0.6, sum);
    }

    #[test]
    fn test_aggregate_sum_empty() {
        let aggregator = TestcaseScoreAggregator::Sum;
        let sum = aggregator.aggregate(vec![]);
        assert_abs_diff_eq!(1.0, sum);
    }

    #[test]
    fn test_input_generator_static() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("input.txt");
        std::fs::write(&path, "x").unwrap();
        let generator = InputGenerator::StaticFile(path.clone());
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new();
        let out = generator.generate(&task, &mut eval, 0, 0).unwrap();
        assert!(eval.dag.data.provided_files.contains_key(&out));
        assert!(eval
            .dag
            .file_callbacks
            .get(&out)
            .unwrap()
            .write_to
            .is_some());
    }

    #[test]
    fn test_input_generator_static_not_found() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("input.txt");
        let generator = InputGenerator::StaticFile(path.clone());
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new();
        let gen = generator.generate(&task, &mut eval, 0, 0);
        assert!(gen.is_err());
        let err = gen.unwrap_err().to_string();
        assert!(err.contains("COPY"));
        assert!(err.contains(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn test_input_generator_custom() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("gen.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let generator = InputGenerator::Custom(Arc::new(source), vec![]);
        let task = make_task(tmpdir.path());
        let (mut eval, _recv) = EvaluationData::new();
        let out = generator.generate(&task, &mut eval, 0, 0).unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 1);
        assert_eq!(eval.dag.data.executions.len(), 1);
        let exec = eval.dag.data.executions.values().next().unwrap();
        assert_eq!(exec.tag.as_ref().unwrap(), &Tag::Generation.into());
        assert_eq!(exec.stdout.as_ref().unwrap().uuid, out);
        assert!(eval
            .dag
            .file_callbacks
            .get(&out)
            .unwrap()
            .write_to
            .is_some());
    }

    #[test]
    fn test_input_validator_assume_valid() {
        let validator = InputValidator::AssumeValid;
        let file = File::new("input");
        let (mut eval, _recv) = EvaluationData::new();
        let out = validator.validate(&mut eval, 0, 0, file.uuid).unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 0);
        assert_eq!(eval.dag.data.executions.len(), 0);
        assert!(out.is_none());
    }

    #[test]
    fn test_input_validator_custom() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("val.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let validator = InputValidator::Custom(Arc::new(source), vec![]);
        let file = File::new("input");
        let (mut eval, _recv) = EvaluationData::new();
        let out = validator.validate(&mut eval, 0, 0, file.uuid).unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 1);
        assert_eq!(eval.dag.data.executions.len(), 1);
        let exec = eval.dag.data.executions.values().next().unwrap();
        assert_eq!(exec.tag.as_ref().unwrap(), &Tag::Generation.into());
        assert_eq!(exec.stdout.as_ref().unwrap().uuid, out.unwrap());
        assert_eq!(exec.env["TM_SUBTASK"], "0");
        assert_eq!(exec.env["TM_TESTCASE"], "0");
    }

    #[test]
    fn test_output_generator_static() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("output.txt");
        std::fs::write(&path, "x").unwrap();
        let generator = OutputGenerator::StaticFile(path.clone());
        let file = File::new("input");
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new();
        let out = generator
            .generate(&task, &mut eval, 0, 0, file.uuid, None)
            .unwrap();
        assert!(eval.dag.data.provided_files.contains_key(&out));
        assert!(eval
            .dag
            .file_callbacks
            .get(&out)
            .unwrap()
            .write_to
            .is_some());
    }

    #[test]
    fn test_output_generator_static_not_found() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("output.txt");
        let generator = OutputGenerator::StaticFile(path.clone());
        let file = File::new("input");
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new();
        let gen = generator.generate(&task, &mut eval, 0, 0, file.uuid, None);
        assert!(gen.is_err());
        let err = gen.unwrap_err().to_string();
        assert!(err.contains("Static output file not found"));
        assert!(err.contains(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn test_output_generator_custom() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("sol.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let generator = OutputGenerator::Custom(Arc::new(source), vec![]);
        let file = File::new("input");
        let val = File::new("validation");
        let task = make_task(tmpdir.path());
        let (mut eval, _recv) = EvaluationData::new();
        let out = generator
            .generate(&task, &mut eval, 0, 0, file.uuid, Some(val.uuid))
            .unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 1);
        assert_eq!(eval.dag.data.executions.len(), 1);
        let exec = eval.dag.data.executions.values().next().unwrap();
        assert_eq!(exec.tag.as_ref().unwrap(), &Tag::Generation.into());
        assert_eq!(exec.stdout.as_ref().unwrap().uuid, out);
        assert!(exec.dependencies().contains(&file.uuid));
        assert!(exec.dependencies().contains(&val.uuid));
        assert!(eval
            .dag
            .file_callbacks
            .get(&out)
            .unwrap()
            .write_to
            .is_some());
    }

    #[test]
    fn test_checker_whitediff() {
        let checker = Checker::WhiteDiff;
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, |_, _| {
                panic!("the callback should not be called here")
            })
            .unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 0);
        assert_eq!(eval.dag.data.executions.len(), 1);
        let exec = eval.dag.data.executions.values().next().unwrap();
        assert_eq!(exec.tag.as_ref().unwrap(), &Tag::Checking.into());
        assert!(exec.args.contains(&"--ignore-all-space".into()));
        assert!(exec.dependencies().contains(&output));
        assert!(exec.dependencies().contains(&test));
    }

    #[test]
    fn test_checker_whitediff_correct() {
        let checker = Checker::WhiteDiff;
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb_called = Arc::new(AtomicBool::new(false));
        let cb_called2 = cb_called.clone();
        let cb = move |score, mex| {
            assert_abs_diff_eq!(score, 1.0);
            assert_eq!(mex, "Output is correct");
            cb_called2.store(true, Ordering::Relaxed);
            Ok(())
        };
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, cb)
            .unwrap();
        let callbacks = eval.dag.execution_callbacks.into_iter().next().unwrap().1;
        callbacks.on_done.into_iter().for_each(|cb| {
            cb.call(ExecutionResult {
                status: ExecutionStatus::Success,
                was_killed: false,
                was_cached: false,
                resources: ExecutionResourcesUsage {
                    cpu_time: 0.0,
                    sys_time: 0.0,
                    wall_time: 0.0,
                    memory: 0,
                },
            })
            .unwrap();
        });
        assert!(cb_called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checker_whitediff_incorrect() {
        let checker = Checker::WhiteDiff;
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb_called = Arc::new(AtomicBool::new(false));
        let cb_called2 = cb_called.clone();
        let cb = move |score, mex| {
            assert_abs_diff_eq!(score, 0.0);
            assert_eq!(mex, "Output is incorrect");
            cb_called2.store(true, Ordering::Relaxed);
            Ok(())
        };
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, cb)
            .unwrap();
        let callbacks = eval.dag.execution_callbacks.into_iter().next().unwrap().1;
        callbacks.on_done.into_iter().for_each(|cb| {
            cb.call(ExecutionResult {
                status: ExecutionStatus::ReturnCode(1),
                was_killed: false,
                was_cached: false,
                resources: ExecutionResourcesUsage {
                    cpu_time: 0.0,
                    sys_time: 0.0,
                    wall_time: 0.0,
                    memory: 0,
                },
            })
            .unwrap();
        });
        assert!(cb_called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checker_custom() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("check.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let checker = Checker::Custom(Arc::new(source));
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, |_, _| {
                panic!("the callback should not be called here")
            })
            .unwrap();
        assert_eq!(eval.dag.data.provided_files.len(), 1);
        assert_eq!(eval.dag.data.executions.len(), 1);
        let exec = eval.dag.data.executions.values().next().unwrap();
        assert_eq!(exec.tag.as_ref().unwrap(), &Tag::Checking.into());
        assert!(exec.dependencies().contains(&input));
        assert!(exec.dependencies().contains(&output));
        assert!(exec.dependencies().contains(&test));
    }

    #[test]
    fn test_checker_custom_correct() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("check.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let checker = Checker::Custom(Arc::new(source));
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb_called = Arc::new(AtomicBool::new(false));
        let cb_called2 = cb_called.clone();
        let cb = move |score, mex| {
            assert_abs_diff_eq!(score, 1.0);
            assert_eq!(mex, "Ok!");
            cb_called2.store(true, Ordering::Relaxed);
            Ok(())
        };
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, cb)
            .unwrap();
        let exec = eval.dag.data.executions.values().next().unwrap();

        let stdout = exec.stdout.as_ref().unwrap().uuid;
        let stdout = eval.dag.file_callbacks.remove(&stdout).unwrap();
        stdout.get_content.unwrap().1.call(b"1.0".to_vec()).unwrap();
        let stderr = exec.stderr.as_ref().unwrap().uuid;
        let stderr = eval.dag.file_callbacks.remove(&stderr).unwrap();
        stderr.get_content.unwrap().1.call(b"Ok!".to_vec()).unwrap();

        assert!(cb_called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checker_custom_incorrect() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("check.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let checker = Checker::Custom(Arc::new(source));
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb_called = Arc::new(AtomicBool::new(false));
        let cb_called2 = cb_called.clone();
        let cb = move |score, mex| {
            assert_abs_diff_eq!(score, 0.0);
            assert_eq!(mex, "Ko!");
            cb_called2.store(true, Ordering::Relaxed);
            Ok(())
        };
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, cb)
            .unwrap();
        let exec = eval.dag.data.executions.values().next().unwrap();

        let stdout = exec.stdout.as_ref().unwrap().uuid;
        let stdout = eval.dag.file_callbacks.remove(&stdout).unwrap();
        stdout.get_content.unwrap().1.call(b"0.0".to_vec()).unwrap();
        let stderr = exec.stderr.as_ref().unwrap().uuid;
        let stderr = eval.dag.file_callbacks.remove(&stderr).unwrap();
        stderr.get_content.unwrap().1.call(b"Ko!".to_vec()).unwrap();

        assert!(cb_called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checker_custom_invalid_score() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("check.py");
        std::fs::write(&path, "x").unwrap();
        let source = SourceFile::new(&path, "", None, None::<PathBuf>).unwrap();
        let checker = Checker::Custom(Arc::new(source));
        let (mut eval, _recv) = EvaluationData::new();
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb = move |_, _| panic!("the callback should not be called here");
        checker
            .check(&mut eval, 0, 0, "sol", input, output, test, cb)
            .unwrap();
        let exec = eval.dag.data.executions.values().next().unwrap();

        let stdout = exec.stdout.as_ref().unwrap().uuid;
        let stdout = eval.dag.file_callbacks.remove(&stdout).unwrap();
        let err = stdout
            .get_content
            .unwrap()
            .1
            .call(b":<".to_vec())
            .unwrap_err()
            .to_string();
        assert!(err.contains("Invalid score from checker"));
        let stderr = exec.stderr.as_ref().unwrap().uuid;
        let stderr = eval.dag.file_callbacks.remove(&stderr).unwrap();
        stderr.get_content.unwrap().1.call(b"Ko!".to_vec()).unwrap();
    }
}
