use crate::task_types::*;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

mod min;

pub use min::*;

/// A score type is the aggregation function for the testcases scores. From the
/// testcases score this is able to compute the subtask scores and the task
/// total score.
pub trait ScoreType<
    SubtaskId: Eq + PartialOrd + Hash + Copy,
    TestcaseId: Eq + PartialOrd + Hash + Copy,
>: Debug
{
    /// Tell the ScoreType the score of a new testcase. Will return true if the
    /// score of a subtask has become ready.
    fn testcase_score(&mut self, subtask: SubtaskId, testcase: TestcaseId, score: f64);

    /// Set the callback that will be called when a subtask has the score
    /// ready.
    fn get_subtask_score(&mut self, callback: Box<Fn(SubtaskId, f64) -> ()>);

    /// Set the callback that will be called when the task has the score ready.
    fn get_task_score(&mut self, callback: Box<Fn(f64) -> ()>);

    /// Clone this ScoreType inside a Box. Note that the callbacks will be
    /// resetted in the result.
    fn boxed(&self) -> Box<dyn ScoreType<SubtaskId, TestcaseId>>;
}

/// Basic data common to all the score types.
struct ScoreTypeBase<
    SubtaskId: Eq + PartialOrd + Hash + Copy + Debug,
    TestcaseId: Eq + PartialOrd + Hash + Copy + Debug,
> {
    /// The total score for this task.
    task_score: Option<f64>,
    /// The score of each subtask.
    subtask_scores: HashMap<SubtaskId, Option<f64>>,
    /// The maximum score of each subtask.
    max_subtask_scores: HashMap<SubtaskId, f64>,
    /// The score of each testcase.
    testcase_scores: HashMap<SubtaskId, HashMap<TestcaseId, Option<f64>>>,
    /// The callback that will be called when a new subtask has the score ready.
    get_subtask_callback: Option<Box<Fn(SubtaskId, f64) -> ()>>,
    /// The callback that will be called when the total score is ready.
    get_task_callback: Option<Box<Fn(f64) -> ()>>,
}

impl<SubtaskId, TestcaseId> ScoreTypeBase<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy + Debug,
    TestcaseId: Eq + PartialOrd + Hash + Copy + Debug,
{
    /// Crate a new ScoreTypeBase from the specified subtask and testcase
    /// information.
    fn new(
        subtasks: HashMap<SubtaskId, &SubtaskInfo>,
        testcases: HashMap<SubtaskId, HashMap<TestcaseId, &TestcaseInfo<SubtaskId, TestcaseId>>>,
    ) -> ScoreTypeBase<SubtaskId, TestcaseId> {
        ScoreTypeBase {
            task_score: None,
            subtask_scores: subtasks.keys().map(|id| (*id, None)).collect(),
            max_subtask_scores: subtasks
                .into_iter()
                .map(|(id, info)| (id, info.max_score()))
                .collect(),
            testcase_scores: testcases
                .into_iter()
                .map(|(st_id, st)| (st_id, st.keys().map(|id| (*id, None)).collect()))
                .collect(),
            get_subtask_callback: None,
            get_task_callback: None,
        }
    }

    /// Set the score of the specified testcase.
    fn testcase_score(&mut self, subtask: SubtaskId, testcase: TestcaseId, score: f64) {
        let stored_score = self
            .testcase_scores
            .get_mut(&subtask)
            .expect("Unknown subtask")
            .get_mut(&testcase)
            .expect("Unknown testcase");
        assert!(stored_score.is_none());
        *stored_score = Some(score);
    }

    /// Set the callback that will be called when a subtask has the score
    /// ready.
    fn get_subtask_score(&mut self, callback: Box<Fn(SubtaskId, f64) -> ()>) {
        self.get_subtask_callback = Some(callback);
    }

    /// Set the callback that will be called when the task has the score ready.
    fn get_task_score(&mut self, callback: Box<Fn(f64) -> ()>) {
        self.get_task_callback = Some(callback);
    }

    /// Make a clone of Self resetting the callbacks (they are not clonable).
    fn partial_clone(&self) -> Self {
        Self {
            task_score: self.task_score,
            subtask_scores: self.subtask_scores.clone(),
            max_subtask_scores: self.max_subtask_scores.clone(),
            testcase_scores: self.testcase_scores.clone(),
            get_subtask_callback: None,
            get_task_callback: None,
        }
    }
}

impl<
        SubtaskId: Eq + PartialOrd + Hash + Copy + Debug,
        TestcaseId: Eq + PartialOrd + Hash + Copy + Debug,
    > std::fmt::Debug for ScoreTypeBase<SubtaskId, TestcaseId>
{
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        // the callbacks are not Debug.
        fmt.debug_struct("ScoreTypeBase")
            .field("task_score", &self.task_score)
            .field("subtask_score", &self.subtask_scores)
            .field("max_subtask_scores", &self.max_subtask_scores)
            .field("testcase_scores", &self.testcase_scores)
            .finish()
    }
}
