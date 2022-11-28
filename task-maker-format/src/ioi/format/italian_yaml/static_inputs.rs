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
    V: Fn(Option<SubtaskId>) -> InputValidator,
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
    V: Fn(Option<SubtaskId>) -> InputValidator,
    O: Fn(TestcaseId) -> OutputGenerator,
{
    type Item = TaskInputEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // the first iteration will emit the subtask entry
        if self.index == 0 {
            self.index = 1;
            return Some(TaskInputEntry::Subtask(SubtaskInfo {
                id: 0,
                name: None,
                description: Some("Static testcases".into()),
                max_score: 100.0,
                testcases: HashMap::new(),
                span: None,
            }));
        }
        let id = self.index - 1; // offset caused by the first iteration
        let path = self.path.join(format!("input{}.txt", id));
        if path.exists() {
            self.index += 1;
            Some(TaskInputEntry::Testcase(TestcaseInfo {
                id,
                input_generator: InputGenerator::StaticFile(path),
                input_validator: (self.get_validator)(Some(0)),
                output_generator: (self.get_output_gen)(id),
            }))
        } else {
            None
        }
    }
}

/// Make a new iterator of the static input files inside the `input/` directory relative to the task
/// root.
///
/// - `get_validator` is a function that, given the id of a subtask, returns the input validator
/// - `get_output_get` is a function that, given the id of a testcase, returns the output generator
pub(crate) fn static_inputs<P: Into<PathBuf>, V, O>(
    task_dir: P,
    get_validator: V,
    get_output_gen: O,
) -> Box<dyn Iterator<Item = TaskInputEntry>>
where
    V: Fn(Option<SubtaskId>) -> InputValidator + 'static,
    O: Fn(TestcaseId) -> OutputGenerator + 'static,
{
    Box::new(StaticInputIter {
        path: task_dir.into().join("input"),
        index: 0,
        get_validator,
        get_output_gen,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use TaskInputEntry::*;

    use crate::ioi::format::italian_yaml::TaskInputEntry;
    use crate::ioi::{InputValidator, OutputGenerator, SubtaskId, TestcaseId};

    use super::*;

    fn get_validator(_subtask: Option<SubtaskId>) -> InputValidator {
        InputValidator::AssumeValid
    }

    fn get_output_generator(_testcase: TestcaseId) -> OutputGenerator {
        OutputGenerator::StaticFile(PathBuf::from("foooo"))
    }

    fn make_task<I: IntoIterator<Item = N>, N: std::fmt::Display>(iter: I) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("input")).unwrap();
        for tc in iter.into_iter() {
            fs::write(
                dir.path().join("input").join(format!("input{}.txt", tc)),
                "",
            )
            .unwrap();
        }
        dir
    }

    #[test]
    fn test_some_inputs() {
        let task = make_task([0, 1, 2]);
        let entries: Vec<_> =
            static_inputs(task.path(), get_validator, get_output_generator).collect();
        if let [Subtask(subtask), Testcase(testcase0), Testcase(testcase1), Testcase(testcase2)] =
            entries.as_slice()
        {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            match &testcase0.input_generator {
                InputGenerator::StaticFile(path) => {
                    assert_eq!(path, &task.path().join("input/input0.txt"))
                }
                InputGenerator::Custom(_, _) => panic!("Invalid generator"),
            }
            match &testcase1.input_generator {
                InputGenerator::StaticFile(path) => {
                    assert_eq!(path, &task.path().join("input/input1.txt"))
                }
                InputGenerator::Custom(_, _) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::StaticFile(path) => {
                    assert_eq!(path, &task.path().join("input/input2.txt"))
                }
                InputGenerator::Custom(_, _) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_no_input() {
        let task = make_task(Vec::<i32>::new());
        let entries: Vec<_> =
            static_inputs(task.path(), get_validator, get_output_generator).collect();
        if let [Subtask(subtask)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }
}
