use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use failure::{format_err, Error};
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
) -> Result<Box<dyn Iterator<Item = TaskInputEntry>>, Error>
where
    V: Fn(SubtaskId) -> InputValidator,
    O: Fn(TestcaseId) -> OutputGenerator,
{
    let task_dir = path.as_ref().parent().unwrap().parent().unwrap();
    let content = std::fs::read_to_string(&path)?;
    let mut file = parser::GENParser::parse(parser::Rule::file, &content)?;
    let file = file.next().unwrap(); // extract the real file
    let mut testcase_count = 0;
    let mut subtask_id: SubtaskId = 0;
    let mut entries = vec![];

    let mut default_subtask = Some(SubtaskInfo {
        id: 0,
        max_score: 100.0,
        testcases: HashMap::new(),
    });

    let generator = find_source_file(
        task_dir,
        vec![
            "gen/generator.*",
            "gen/generatore.*",
            "gen/generator",
            "gen/generatore",
        ],
        None,
    )
    .map(Arc::new)
    .ok_or_else(|| format_err!("No generator found"))?;
    debug!("Detected input generator: {:?}", generator);

    for line in file.into_inner() {
        match line.as_rule() {
            parser::Rule::line => {
                let line = line.into_inner().next().unwrap();
                match line.as_rule() {
                    parser::Rule::subtask => {
                        default_subtask.take(); // ignore the default subtask ever
                        let score = line.into_inner().next().unwrap().as_str();
                        entries.push(TaskInputEntry::Subtask(SubtaskInfo {
                            id: subtask_id,
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
                        let what = line.into_inner().next().unwrap().as_str();
                        entries.push(TaskInputEntry::Testcase(TestcaseInfo {
                            id: testcase_count,
                            input_generator: InputGenerator::StaticFile(what.into()),
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
                        entries.push(TaskInputEntry::Testcase(TestcaseInfo {
                            id: testcase_count,
                            input_generator: InputGenerator::Custom(generator.clone(), cmd),
                            input_validator: get_validator(subtask_id - 1),
                            output_generator: get_output_gen(testcase_count),
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
    Ok(Box::new(entries.into_iter()))
}
