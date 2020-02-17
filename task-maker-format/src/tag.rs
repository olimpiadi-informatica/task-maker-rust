use task_maker_dag::ExecutionTag;

lazy_static! {
    /// The list of all the ExecutionTags used for the evaluation.
    pub static ref VALID_TAGS: Vec<String> = [
        "compilation",
        "generation",
        "evaluation",
        "checking",
        "booklet"
    ]
    .iter()
    .map(|s| String::from(*s))
    .collect();
}

/// Tags of the various executions inside a IOI task.
pub enum Tag {
    /// Generation of a testcase.
    Generation,
    /// Evaluation of a solution.
    Evaluation,
    /// Checking of a solution.
    Checking,
    /// Compilation of the booklet.
    Booklet,
}

impl Into<ExecutionTag> for Tag {
    fn into(self) -> ExecutionTag {
        match self {
            Tag::Generation => ExecutionTag::from("generation"),
            Tag::Evaluation => ExecutionTag::from("evaluation"),
            Tag::Checking => ExecutionTag::from("checking"),
            Tag::Booklet => ExecutionTag::from("booklet"),
        }
    }
}
