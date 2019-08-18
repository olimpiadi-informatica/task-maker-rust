use crate::task_types::ioi::formats::TaskInputEntry;
use crate::task_types::ioi::*;
use failure::Error;
use std::path::{Path, PathBuf};

/// The iterator over the static input files
struct StaticInputIter {
    /// The path to the input files directory.
    path: PathBuf,
    /// The index of the next input file.
    index: u32,
}

impl Iterator for StaticInputIter {
    type Item = TaskInputEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // the first iteration will emit the subtask entry
        if self.index == 0 {
            self.index = 1;
            return Some(TaskInputEntry::Subtask {
                id: 0,
                info: IOISubtaskInfo { max_score: 100.0 },
            });
        }
        let id = self.index - 1;
        let path = self.path.join(format!("input{}.txt", id));
        if path.exists() {
            self.index += 1;
            Some(TaskInputEntry::CopyTestcase {
                subtask: 0,
                id,
                path,
            })
        } else {
            None
        }
    }
}

/// Make an iterator over all the input files inside the input/ folder. The
/// files should be named inputX.txt where X is an integer starting from zero.
pub fn static_inputs(path: &Path) -> Result<Box<Iterator<Item = TaskInputEntry>>, Error> {
    Ok(Box::new(StaticInputIter {
        path: path.join("input").to_owned(),
        index: 0,
    }))
}
