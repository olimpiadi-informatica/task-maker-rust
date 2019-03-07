use crate::task_types::ioi::*;
use failure::Error;
use pest::Parser;
use std::collections::HashMap;
use std::path::Path;

#[derive(Parser)]
#[grammar = "task_types/ioi/formats/GEN.pest"]
struct GENParser;

/// Parse the gen/GEN file extracting the subtasks and the testcases
pub fn parse_gen_gen(
    path: &Path,
) -> Result<
    (
        HashMap<IOISubtaskId, IOISubtaskInfo>,
        HashMap<IOISubtaskId, HashMap<IOITestcaseId, IOITestcaseInfo>>,
    ),
    Error,
> {
    let content = std::fs::read_to_string(path)?;
    let mut file = GENParser::parse(Rule::file, &content)?;
    let file = file.next().unwrap(); // extract the real file
    let mut subtasks = HashMap::new();
    let mut testcases = HashMap::new();
    let mut last_subtask = None;
    let mut last_testcases: HashMap<IOITestcaseId, IOITestcaseInfo> = HashMap::new();
    let mut testcase_count = 0;

    let mut default_subtask = Some(IOISubtaskInfo {
        max_score: 100.0,
        score_mode: "max".to_string(),
    });

    for line in file.into_inner() {
        match line.as_rule() {
            Rule::line => {
                let line = line.into_inner().next().unwrap();
                match line.as_rule() {
                    Rule::subtask => {
                        if let Some(subtask) = last_subtask {
                            let subtask_id = subtasks.len() as IOISubtaskId;
                            subtasks.insert(subtask_id, subtask);
                            testcases.insert(subtask_id, last_testcases);
                            last_testcases = HashMap::new();
                        }
                        let score = line.into_inner().next().unwrap().as_str();
                        last_subtask = Some(IOISubtaskInfo {
                            max_score: score.parse::<f64>().unwrap(),
                            score_mode: "max".to_string(), // TODO
                        });
                    }
                    Rule::copy => {
                        if last_subtask.is_none() {
                            last_subtask = default_subtask.take();
                        }
                        let what = line.into_inner().next().unwrap().as_str();
                        last_testcases.insert(
                            testcase_count,
                            IOITestcaseInfo {
                                testcase: testcase_count,
                                static_input: Some(std::path::PathBuf::from(what)),
                                static_output: None,
                                generator: (),
                                validator: (),
                            },
                        );
                        testcase_count += 1;
                    }
                    Rule::command => {
                        if last_subtask.is_none() {
                            last_subtask = default_subtask.take();
                        }
                        let cmd: Vec<String> =
                            line.into_inner().map(|x| x.as_str().to_owned()).collect();
                        last_testcases.insert(
                            testcase_count,
                            IOITestcaseInfo {
                                testcase: testcase_count,
                                static_input: None,
                                static_output: None,
                                generator: (), // TODO
                                validator: (),
                            },
                        );
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
    let subtask_id = subtasks.len() as IOISubtaskId;
    subtasks.insert(subtask_id, last_subtask.unwrap());
    testcases.insert(subtask_id, last_testcases);
    Ok((subtasks, testcases))
}
