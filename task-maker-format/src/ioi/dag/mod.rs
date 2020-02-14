use serde::{Deserialize, Serialize};

pub use checker::Checker;
pub use input_generator::InputGenerator;
pub use input_validator::InputValidator;
pub use output_generator::OutputGenerator;
pub use task_type::TaskType;

mod checker;
mod input_generator;
mod input_validator;
mod output_generator;
mod task_type;

/// Maximum number of bytes of the captured standard error.
pub const STDERR_CONTENT_LENGTH: usize = 10 * 1024;

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
#[macro_export]
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
    use std::sync::atomic::{AtomicBool, Ordering};

    use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult, ExecutionStatus, File};
    use task_maker_lang::GraderMap;

    use super::*;
    use crate::ioi::{Tag, Task};
    use crate::{EvaluationData, SourceFile};
    use std::path::PathBuf;
    use std::sync::Arc;

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
            input_validator: InputValidator::AssumeValid,
            output_generator: None,
            checker: Checker::WhiteDiff,
            testcase_score_aggregator: TestcaseScoreAggregator::Min,
            grader_map: Arc::new(GraderMap::new(Vec::<PathBuf>::new())),
            booklets: vec![],
            difficulty: None,
            syllabus_level: None,
            sanity_checks: Default::default(),
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
        let generator = InputGenerator::StaticFile(path);
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new(tmpdir.path());
        let out = generator.generate_and_bind(&task, &mut eval, 0, 0).unwrap();
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
        let (mut eval, _) = EvaluationData::new(tmpdir.path());
        let gen = generator.generate_and_bind(&task, &mut eval, 0, 0);
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
        let out = generator.generate_and_bind(&task, &mut eval, 0, 0).unwrap();
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
        let (mut eval, _recv) = EvaluationData::new("");
        let out = validator
            .validate_and_bind(&mut eval, 0, 0, file.uuid)
            .unwrap();
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
        let out = validator
            .validate_and_bind(&mut eval, 0, 0, file.uuid)
            .unwrap();
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
        let generator = OutputGenerator::StaticFile(path);
        let file = File::new("input");
        let task = make_task(tmpdir.path());
        let (mut eval, _) = EvaluationData::new(tmpdir.path());
        let out = generator
            .generate_and_bind(&task, &mut eval, 0, 0, file.uuid, None)
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
        let (mut eval, _) = EvaluationData::new(tmpdir.path());
        let gen = generator.generate_and_bind(&task, &mut eval, 0, 0, file.uuid, None);
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
        let out = generator
            .generate_and_bind(&task, &mut eval, 0, 0, file.uuid, Some(val.uuid))
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
        let (mut eval, _recv) = EvaluationData::new("");
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        checker
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, |_, _| {
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
        let (mut eval, _recv) = EvaluationData::new("");
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
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, cb)
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
        let (mut eval, _recv) = EvaluationData::new("");
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
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, cb)
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        checker
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, |_, _| {
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
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
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, cb)
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
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
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, cb)
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
        let (mut eval, _recv) = EvaluationData::new(tmpdir.path());
        let input = File::new("input").uuid;
        let output = File::new("output").uuid;
        let test = File::new("test").uuid;
        let cb = move |_, _| panic!("the callback should not be called here");
        checker
            .check_and_bind(&mut eval, 0, 0, "sol", input, output, test, cb)
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
