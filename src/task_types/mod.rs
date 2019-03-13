use crate::evaluation::*;
use crate::execution::*;
use crate::executor::*;
use crate::score_types::*;
use crate::ui::*;
use failure::Error;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

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
    fn generate(
        &self,
        eval: &mut EvaluationData,
        subtask: SubtaskId,
        testcase: TestcaseId,
    ) -> (File, Option<Execution>);
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
        eval: &mut EvaluationData,
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
    /// output file. The score_type parameter is useful for setting the score
    /// to zero if the solution failed (i.e. crashed). This method is required
    /// to set the score for this testcase if and only if the checker won't
    /// run.
    fn solve(
        &self,
        eval: &mut EvaluationData,
        input: File,
        validation: Option<File>,
        subtask: SubtaskId,
        testcase: TestcaseId,
        score_type: Option<Arc<Mutex<Box<dyn ScoreType<SubtaskId, TestcaseId>>>>>,
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
        eval: &mut EvaluationData,
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

/// An interface between a Task type and the UI. Each task type may send
/// different information to the UI based on the number of the testcase.
pub trait TaskUIInterface<
    SubtaskId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
    TestcaseId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
>
{
    /// Send to the UI the status of the generation of a testcase.
    fn generation_result(
        &self,
        sender: Arc<Mutex<UIMessageSender>>,
        subtask: SubtaskId,
        testcase: TestcaseId,
        status: UIExecutionStatus,
    );
}

/// Trait that describes a generic task. Every task must have a generator (a
/// way of getting testcases) and can have a validator, an official solution,
/// but has to have a checker that assigns a score to a solution.
pub trait Task<
    SubtaskId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
    TestcaseId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
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

    /// Get the TaskUIInterface relative to this
    fn get_ui_interface(&self) -> Arc<TaskUIInterface<SubtaskId, TestcaseId>>;

    /// Build the DAG of the evaluation of this task and use the executor to
    /// start the evaluation. This method will block until the evaluation ends.
    fn evaluate(
        &self,
        mut eval: EvaluationData,
        options: &EvaluationOptions,
        mut executor: LocalExecutor,
    ) {
        let subtasks = self.subtasks();
        let solutions = self.solutions();
        // the scores of the solutions, the values must be thread-safe because
        // they are changed in other threads during the evaluation.
        let solutions_scores: HashMap<
            PathBuf,
            Arc<Mutex<Box<dyn ScoreType<SubtaskId, TestcaseId>>>>,
        > = solutions
            .keys()
            .map(|sol| {
                let mut score_type = self.score_type().boxed();
                let name1 = sol.clone();
                let name2 = sol.clone();
                score_type.get_subtask_score(Box::new(move |subtask, score| {
                    warn!("Score of {:?} subtask {:?} is {}", name1, subtask, score);
                }));
                score_type.get_task_score(Box::new(move |score| {
                    warn!("Score of {:?} task is {}", name2, score);
                }));
                (sol.clone(), Arc::new(Mutex::new(score_type)))
            })
            .collect();

        for (st_num, _st) in subtasks.iter() {
            for (tc_num, tc) in self.testcases(*st_num).iter() {
                let interface = self.get_ui_interface().clone();
                // STEP 1: generate the input file
                let (input, exec) = tc.generator().generate(&mut eval, *st_num, *tc_num);
                if let Some(path) = tc.write_input_to() {
                    if !options.dry_run() {
                        eval.dag.write_file_to(&input, &self.path().join(path));
                    }
                }
                if let Some(exec) = exec {
                    bind_generation_callbacks(&interface, exec, &mut eval, *st_num, *tc_num);
                }

                // STEP 2: validate the input file if there is a validator
                let val = if let Some(validator) = tc.validator() {
                    Some(validator.validate(&mut eval, input.clone(), *st_num, *tc_num))
                } else {
                    None
                };

                // STEP 3: generate the output file if there is an official solutions
                let output = if let Some(solution) = self.official_solution(*st_num, *tc_num) {
                    Some(solution.solve(
                        &mut eval,
                        input.clone(),
                        val.as_ref().map(|f| f.clone()),
                        *st_num,
                        *tc_num,
                        None,
                    ))
                } else {
                    None
                };
                if let (Some(ref output), Some(ref path)) = (&output, &tc.write_output_to()) {
                    if !options.dry_run() {
                        eval.dag.write_file_to(&output, &self.path().join(path));
                    }
                }

                // STEP 4: evaluate all the solutions on this testcase
                for (sol_path, sol) in solutions.iter() {
                    let score_type = solutions_scores.get(sol_path).unwrap().clone();
                    // STEP 4a: execute the solution with the input file
                    let sol_output = sol.solve(
                        &mut eval,
                        input.clone(),
                        val.clone(),
                        *st_num,
                        *tc_num,
                        Some(score_type.clone()),
                    );
                    let sol_path2 = sol_path.clone();
                    let st_num2 = *st_num;
                    let tc_num2 = *tc_num;
                    // STEP 4b: run the checker on the outcome and store the result.
                    self.checker(*st_num, *tc_num).check(
                        &mut eval,
                        input.clone(),
                        output.clone(),
                        sol_output,
                        *st_num,
                        *tc_num,
                        Box::new(move |res| {
                            let mut score_type = score_type.lock().unwrap();
                            score_type.testcase_score(st_num2, tc_num2, res.score);
                            warn!(
                                "Score of {:?} at testcase {:?} is {}",
                                sol_path2, tc_num2, res.score
                            );
                        }),
                    );
                }
            }
        }

        let (tx, rx_remote) = channel();
        let (tx_remote, rx) = channel();
        let server = thread::spawn(move || {
            executor.evaluate(tx_remote, rx_remote).unwrap();
        });
        ExecutorClient::evaluate(eval, tx, rx).unwrap();
        server.join().expect("Server paniced");
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

/// Bind the callbacks relative to the generation execution.
fn bind_generation_callbacks<SubtaskId, TestcaseId>(
    interface: &Arc<TaskUIInterface<SubtaskId, TestcaseId>>,
    exec: Execution,
    eval: &mut EvaluationData,
    st_num: SubtaskId,
    tc_num: TestcaseId,
) where
    SubtaskId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
    TestcaseId: Eq + PartialOrd + Hash + Copy + std::fmt::Debug + 'static,
{
    let interface1 = interface.clone();
    let interface2 = interface.clone();
    let interface3 = interface.clone();
    let (sender1, st_num1, tc_num1) = (eval.sender.clone(), st_num, tc_num);
    let (sender2, st_num2, tc_num2) = (eval.sender.clone(), st_num, tc_num);
    let (sender3, st_num3, tc_num3) = (eval.sender.clone(), st_num, tc_num);
    eval.dag
        .add_execution(exec)
        .on_start(move |worker| {
            interface1.generation_result(
                sender1,
                st_num1,
                tc_num1,
                UIExecutionStatus::Started {
                    worker: worker.to_string(),
                },
            );
        })
        .on_done(move |result| {
            interface2.generation_result(
                sender2,
                st_num2,
                tc_num2,
                UIExecutionStatus::Done { result },
            );
        })
        .on_skip(move || {
            interface3.generation_result(sender3, st_num3, tc_num3, UIExecutionStatus::Skipped);
        });
}
