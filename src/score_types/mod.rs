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
    fn testcase_score(&mut self, subtask: SubtaskId, testcase: TestcaseId, score: f64) -> bool;

    /// Ask the ScoreType the score of a subtask, will return None if the score
    /// is not ready yet.
    fn get_subtask_score(&mut self, subtask: SubtaskId) -> Option<f64>;

    /// Ask the ScoreType the total score of the task, will return None if the
    /// score is not ready yet.
    fn get_task_score(&mut self) -> Option<f64>;

    /// Clone this ScoreType inside a Box
    fn clone(&self) -> Box<dyn ScoreType<SubtaskId, TestcaseId>>;
}

/// Basic data common to all the score types.
#[derive(Debug, Clone)]
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
}

impl<SubtaskId, TestcaseId> ScoreTypeBase<SubtaskId, TestcaseId>
where
    SubtaskId: Eq + PartialOrd + Hash + Copy + Debug,
    TestcaseId: Eq + PartialOrd + Hash + Copy + Debug,
{
    fn new(
        subtasks: HashMap<SubtaskId, &SubtaskInfo>,
        testcases: HashMap<SubtaskId, HashMap<TestcaseId, &TestcaseInfo<SubtaskId, TestcaseId>>>,
    ) -> ScoreTypeBase<SubtaskId, TestcaseId> {
        ScoreTypeBase {
            task_score: None,
            subtask_scores: subtasks.keys().map(|id| (id.clone(), None)).collect(),
            max_subtask_scores: subtasks
                .into_iter()
                .map(|(id, info)| (id, info.max_score()))
                .collect(),
            testcase_scores: testcases
                .into_iter()
                .map(|(st_id, st)| (st_id, st.keys().map(|id| (id.clone(), None)).collect()))
                .collect(),
        }
    }

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

    fn get_subtask_score(&mut self, subtask: SubtaskId) -> Option<f64> {
        self.subtask_scores
            .get(&subtask)
            .expect("Unknown subtask")
            .clone()
    }

    fn get_task_score(&mut self) -> Option<f64> {
        self.task_score.clone()
    }
}
