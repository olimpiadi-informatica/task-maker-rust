use crate::task_types::ioi::formats::TaskInputEntry;
use crate::task_types::ioi::*;
use failure::Error;
use pest::Parser;
use std::collections::HashMap;
use std::path::Path;

#[derive(Parser)]
#[grammar = "task_types/ioi/formats/GEN.pest"]
struct GENParser;

pub type IOISubtasksInfo = HashMap<IOISubtaskId, IOISubtaskInfo>;
pub type IOITestcasesInfo = HashMap<IOISubtaskId, HashMap<IOITestcaseId, IOITestcaseInfo>>;

/// Parse the gen/GEN file extracting the subtasks and the testcases
pub fn parse_gen_gen(path: &Path) -> Result<Box<Iterator<Item = TaskInputEntry>>, Error> {
    let task_dir = path.parent().unwrap().parent().unwrap();
    let content = std::fs::read_to_string(path)?;
    let mut file = GENParser::parse(Rule::file, &content)?;
    let file = file.next().unwrap(); // extract the real file
    let mut last_subtask = None;
    let mut testcase_count = 0;
    let mut subtask_id: IOISubtaskId = 0;
    let mut entries = vec![];

    let mut default_subtask = Some(IOISubtaskInfo { max_score: 100.0 });

    for line in file.into_inner() {
        match line.as_rule() {
            Rule::line => {
                let line = line.into_inner().next().unwrap();
                match line.as_rule() {
                    Rule::subtask => {
                        if let Some(subtask) = last_subtask {
                            entries.push(TaskInputEntry::Subtask {
                                id: subtask_id,
                                info: subtask,
                            });
                            subtask_id += 1;
                        }
                        let score = line.into_inner().next().unwrap().as_str();
                        last_subtask = Some(IOISubtaskInfo {
                            max_score: score.parse::<f64>().unwrap(),
                        });
                    }
                    Rule::copy => {
                        if last_subtask.is_none() {
                            last_subtask = default_subtask.take();
                        }
                        let what = line.into_inner().next().unwrap().as_str();
                        entries.push(TaskInputEntry::CopyTestcase {
                            subtask: subtask_id,
                            id: testcase_count,
                            path: task_dir.join(what),
                        });
                        testcase_count += 1;
                    }
                    Rule::command => {
                        if last_subtask.is_none() {
                            last_subtask = default_subtask.take();
                        }
                        let cmd: Vec<String> =
                            line.into_inner().map(|x| x.as_str().to_owned()).collect();
                        entries.push(TaskInputEntry::GenerateTestcase {
                            subtask: subtask_id,
                            id: testcase_count,
                            cmd,
                        });
                        testcase_count += 1;
                    }
                    Rule::comment => {}
                    Rule::empty => {}
                    _ => unreachable!(),
                }
            }
            Rule::EOI => {}
            _ => unreachable!(),
        }
    }
    entries.push(TaskInputEntry::Subtask {
        id: subtask_id,
        info: last_subtask.unwrap(),
    });
    Ok(Box::new(entries.into_iter()))
}
