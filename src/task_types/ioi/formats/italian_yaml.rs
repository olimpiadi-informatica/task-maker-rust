use crate::task_types::ioi::*;
use crate::task_types::*;
use std::path::Path;

pub struct IOIItalianYaml;

impl TaskFormat for IOIItalianYaml {
    type SubtaskId = IOISubtaskId;
    type TestcaseId = IOITestcaseId;
    type SubtaskInfo = IOISubtaskInfo;
    type TestcaseInfo = IOITestcaseInfo;

    fn is_valid(path: &Path) -> bool {
        false
    }

    fn parse(
        path: &Path,
    ) -> Box<Task<Self::SubtaskId, Self::TestcaseId, Self::SubtaskInfo, Self::TestcaseInfo>> {
        unimplemented!();
    }
}
