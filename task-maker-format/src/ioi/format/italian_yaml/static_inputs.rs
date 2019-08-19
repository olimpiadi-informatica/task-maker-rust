use std::collections::HashMap;
use std::path::PathBuf;

use crate::ioi::format::italian_yaml::TaskInputEntry;
use crate::ioi::{
    InputGenerator, InputValidator, OutputGenerator, SubtaskId, SubtaskInfo, TestcaseId,
    TestcaseInfo,
};

/// The iterator over the static input files. It looks for `input/input{}.txt` starting from zero
/// till it finds the last input. It uses due functions to get the input validator and the output
/// generator.
struct StaticInputIter<V, O>
where
    V: Fn(SubtaskId) -> InputValidator,
    O: Fn(TestcaseId) -> OutputGenerator,
{
    /// The path to the input files directory.
    path: PathBuf,
    /// The index of the next input file.
    index: u32,
    /// The function to use to get the input validator of an input file.
    get_validator: V,
    /// The function to use to get the output generator of an input file.
    get_output_gen: O,
}

impl<V, O> Iterator for StaticInputIter<V, O>
where
    V: Fn(SubtaskId) -> InputValidator,
    O: Fn(TestcaseId) -> OutputGenerator,
{
    type Item = TaskInputEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // the first iteration will emit the subtask entry
        if self.index == 0 {
            self.index = 1;
            return Some(TaskInputEntry::Subtask(SubtaskInfo {
                id: 0,
                max_score: 100.0,
                testcases: HashMap::new(),
            }));
        }
        let id = self.index - 1; // offset caused by the first iteration
        let path = self.path.join(format!("input{}.txt", id));
        if path.exists() {
            self.index += 1;
            Some(TaskInputEntry::Testcase(TestcaseInfo {
                id,
                input_generator: InputGenerator::StaticFile(path),
                input_validator: (self.get_validator)(0),
                output_generator: (self.get_output_gen)(id),
            }))
        } else {
            None
        }
    }
}

/// Make a new iterator of the static input files inside the `input/` directory relative to the task
/// root.
pub(crate) fn static_inputs<P: Into<PathBuf>, V, O>(
    task_dir: P,
    get_validator: V,
    get_output_gen: O,
) -> Box<dyn Iterator<Item = TaskInputEntry>>
where
    V: Fn(SubtaskId) -> InputValidator + 'static,
    O: Fn(TestcaseId) -> OutputGenerator + 'static,
{
    Box::new(StaticInputIter {
        path: task_dir.into().join("input"),
        index: 0,
        get_validator,
        get_output_gen,
    })
}
