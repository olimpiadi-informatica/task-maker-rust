use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Error};
use pest::Parser;

use crate::find_source_file;
use crate::ioi::format::italian_yaml::TaskInputEntry;
use crate::ioi::{
    InputGenerator, InputValidator, OutputGenerator, SubtaskId, SubtaskInfo, TestcaseId,
    TestcaseInfo,
};

/// This module exists because of a `pest`'s bug: https://github.com/pest-parser/pest/issues/326
#[allow(missing_docs)]
mod parser {
    /// The gen/GEN file parser.
    #[derive(Parser)]
    #[grammar = "ioi/format/italian_yaml/GEN.pest"]
    pub struct GENParser;
}

/// Parse the `gen/GEN` file extracting the subtasks and the testcases.
pub(crate) fn parse_gen_gen<P: AsRef<Path>, V, O>(
    path: P,
    get_validator: V,
    get_output_gen: O,
) -> Result<Vec<TaskInputEntry>, Error>
where
    V: Fn(SubtaskId) -> InputValidator,
    O: Fn(TestcaseId) -> OutputGenerator,
{
    let path = path.as_ref();
    let task_dir = path
        .parent()
        .expect("Invalid gen/GEN path")
        .parent()
        .expect("Invalid gen/GEN path");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read gen/GEN from {}", path.display()))?;
    let mut file =
        parser::GENParser::parse(parser::Rule::file, &content).context("Cannot parse gen/GEN")?;
    let file = file.next().ok_or_else(|| anyhow!("Corrupted parser"))?; // extract the real file
    let mut testcase_count = 0;
    let mut subtask_id: SubtaskId = 0;
    let mut entries = vec![];

    let mut default_subtask = Some(SubtaskInfo {
        id: 0,
        description: None,
        max_score: 100.0,
        testcases: HashMap::new(),
    });

    let mut generators = find_source_file(
        task_dir,
        vec![
            "gen/generator.*",
            "gen/generatore.*",
            "gen/generator",
            "gen/generatore",
        ],
        task_dir,
        None,
        Some(task_dir.join("bin").join("generator")),
    );
    if generators.len() > 1 {
        let paths = generators.iter().map(|s| s.name()).collect::<Vec<_>>();
        bail!("Multiple generators found: {:?}", paths);
    } else if generators.is_empty() {
        bail!("No generator found");
    }
    let generator = generators.pop().map(Arc::new).unwrap();
    debug!("Detected input generator: {:?}", generator);

    for line in file.into_inner() {
        match line.as_rule() {
            parser::Rule::line => {
                let line = line
                    .into_inner()
                    .next()
                    .ok_or_else(|| anyhow!("Corrupted parser"))?;
                match line.as_rule() {
                    parser::Rule::subtask => {
                        default_subtask.take(); // ignore the default subtask ever
                        let score = line
                            .into_inner()
                            .next()
                            .ok_or_else(|| anyhow!("Corrupted parser"))?
                            .as_str();
                        entries.push(TaskInputEntry::Subtask(SubtaskInfo {
                            id: subtask_id,
                            description: None,
                            max_score: score.parse::<f64>().expect("Invalid subtask score"),
                            testcases: HashMap::new(),
                        }));
                        subtask_id += 1;
                    }
                    parser::Rule::copy => {
                        if let Some(default) = default_subtask.take() {
                            entries.push(TaskInputEntry::Subtask(default));
                            subtask_id += 1;
                        }
                        let what = line
                            .into_inner()
                            .next()
                            .ok_or_else(|| anyhow!("Corrupted parser"))?
                            .as_str();
                        entries.push(TaskInputEntry::Testcase(TestcaseInfo {
                            id: testcase_count,
                            input_generator: InputGenerator::StaticFile(task_dir.join(what)),
                            input_validator: get_validator(subtask_id - 1),
                            output_generator: get_output_gen(testcase_count),
                        }));
                        testcase_count += 1;
                    }
                    parser::Rule::command => {
                        if let Some(default) = default_subtask.take() {
                            entries.push(TaskInputEntry::Subtask(default));
                            subtask_id += 1;
                        }
                        let cmd: Vec<String> =
                            line.into_inner().map(|x| x.as_str().to_owned()).collect();
                        let output_generator = get_output_gen(testcase_count);
                        if let OutputGenerator::StaticFile(_) = output_generator {
                            bail!("Generator detected but no solution found. Cannot generate output files.");
                        }
                        entries.push(TaskInputEntry::Testcase(TestcaseInfo {
                            id: testcase_count,
                            input_generator: InputGenerator::Custom(generator.clone(), cmd),
                            input_validator: get_validator(subtask_id - 1),
                            output_generator,
                        }));
                        testcase_count += 1;
                    }
                    parser::Rule::comment => {}
                    parser::Rule::empty => {}
                    _ => unreachable!(),
                }
            }
            parser::Rule::EOI => {}
            _ => unreachable!(),
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use crate::ioi::format::italian_yaml::gen_gen::parse_gen_gen;
    use crate::ioi::format::italian_yaml::TaskInputEntry;
    use crate::ioi::{InputGenerator, InputValidator, OutputGenerator, SubtaskId, TestcaseId};
    use crate::SourceFile;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempdir::TempDir;
    use TaskInputEntry::*;

    fn make_task<S: AsRef<str>>(gen_gen: S) -> TempDir {
        let dir = TempDir::new("tm-test").unwrap();
        fs::write(dir.path().join("task.yaml"), "name: foo\ntitle: foo bar\n").unwrap();
        fs::create_dir(dir.path().join("gen")).unwrap();
        fs::create_dir(dir.path().join("sol")).unwrap();
        fs::write(dir.path().join("gen").join("GEN"), gen_gen.as_ref()).unwrap();
        fs::write(
            dir.path().join("gen").join("generator.py"),
            "#!/usr/bin/env python",
        )
        .unwrap();
        fs::write(
            dir.path().join("sol").join("solution.py"),
            "#!/usr/bin/env python",
        )
        .unwrap();
        dir
    }

    fn get_validator(_subtask: SubtaskId) -> InputValidator {
        InputValidator::AssumeValid
    }

    fn get_output_generator(_testcase: TestcaseId) -> OutputGenerator {
        let source = SourceFile::new("a.py", "", None, None::<&str>).unwrap();
        OutputGenerator::Custom(Arc::new(source), vec![])
    }

    fn get_entries(dir: &Path) -> Vec<TaskInputEntry> {
        parse_gen_gen(
            dir.join("gen").join("GEN"),
            get_validator,
            get_output_generator,
        )
        .unwrap()
    }

    #[test]
    fn test_parser_single_line() {
        let task = make_task("1234\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_single_line_without_ending_lf() {
        let task = make_task("1234");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_single_line_with_comments() {
        let task = make_task("# this is a comment\n1234\n# this is a comment too\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_comment_empty() {
        let task = make_task("#\n1234\n#\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_comment_empty_no_ending_lf() {
        let task = make_task("#\n1234\n#");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_line_with_comment() {
        let task = make_task("1234 # normal comment\n5678 #risky comment");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase1), Testcase(testcase2)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase1.id, 0);
            assert_eq!(testcase2.id, 1);
            match &testcase1.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["5678".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_multiple_lines() {
        let task = make_task("1234\n5678\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase1), Testcase(testcase2)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase1.id, 0);
            assert_eq!(testcase2.id, 1);
            match &testcase1.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["5678".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_copy() {
        let task = make_task("#COPY: random/file\n5678\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase1), Testcase(testcase2)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase1.id, 0);
            assert_eq!(testcase2.id, 1);
            match &testcase1.input_generator {
                InputGenerator::StaticFile(path) => {
                    assert_eq!(path, &task.path().join("random/file"))
                }
                InputGenerator::Custom(_, _) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["5678".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_subtasks() {
        let task = make_task("#ST: 123\n#COPY: random/file\n5678\n#ST: 321\n1234\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask1), Testcase(testcase1), Testcase(testcase2), Subtask(subtask2), Testcase(testcase3)] =
            entries.as_slice()
        {
            assert_eq!(subtask1.id, 0);
            assert_eq!(subtask1.max_score as u32, 123);
            assert_eq!(testcase1.id, 0);
            assert_eq!(testcase2.id, 1);
            assert_eq!(subtask2.id, 1);
            assert_eq!(subtask2.max_score as u32, 321);
            match &testcase1.input_generator {
                InputGenerator::StaticFile(path) => {
                    assert_eq!(path, &task.path().join("random/file"))
                }
                InputGenerator::Custom(_, _) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["5678".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
            match &testcase3.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_empty_lines() {
        let task = make_task("\n\n1234\n\n\n5678\n\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase1), Testcase(testcase2)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase1.id, 0);
            assert_eq!(testcase2.id, 1);
            match &testcase1.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
            match &testcase2.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["5678".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }

    #[test]
    fn test_parser_spaces_before_and_after() {
        let task = make_task("  \t 1234\t  \t\n");
        let entries = get_entries(task.path());
        if let [Subtask(subtask), Testcase(testcase)] = entries.as_slice() {
            assert_eq!(subtask.id, 0);
            assert_eq!(subtask.max_score as u32, 100);
            assert_eq!(testcase.id, 0);
            match &testcase.input_generator {
                InputGenerator::Custom(_, args) => assert_eq!(args, &vec!["1234".to_string()]),
                InputGenerator::StaticFile(_) => panic!("Invalid generator"),
            }
        } else {
            panic!("Wrong entries returned: {:?}", entries);
        }
    }
}
