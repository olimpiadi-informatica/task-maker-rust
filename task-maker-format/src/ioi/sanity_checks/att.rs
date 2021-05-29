use std::collections::HashMap;
use std::path::PathBuf;

use failure::{format_err, Error};
use regex::Regex;

use task_maker_dag::File;

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::{IOITask, TaskType, TestcaseId};
use crate::sanity_checks::SanityCheck;
use crate::ui::{UIMessage, UIMessageSender};
use crate::{list_files, EvaluationData, UISender};

/// Check that all the graders are present inside att.
#[derive(Debug, Default)]
pub struct AttGraders;

impl SanityCheck<IOITask> for AttGraders {
    fn name(&self) -> &'static str {
        "AttGraders"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        check_missing_graders(task, eval, "att")
    }
}

/// Check that all the templates are present inside att.
#[derive(Debug, Default)]
pub struct AttTemplates;

impl SanityCheck<IOITask> for AttTemplates {
    fn name(&self) -> &'static str {
        "AttTemplates"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        for grader in task.grader_map.all_paths() {
            let ext = grader
                .extension()
                .ok_or_else(|| format_err!("Grader has no extension"))?
                .to_string_lossy();
            let template = task.path.join("att").join(format!("{}.{}", task.name, ext));
            if !template.exists() {
                eval.sender.send(UIMessage::Warning {
                    message: format!("Missing template at att/{}.{}", task.name, ext),
                })?;
            }
        }
        Ok(())
    }
}

/// Check that the sample cases inside att are valid symlinks.
#[derive(Debug, Default)]
pub struct AttSampleFiles;

impl SanityCheck<IOITask> for AttSampleFiles {
    fn name(&self) -> &'static str {
        "AttSampleFiles"
    }

    fn post_hook(&mut self, task: &IOITask, ui: &mut UIMessageSender) -> Result<(), Error> {
        let mut no_sample = true;
        for sample in list_files(&task.path, vec!["att/*input*.txt", "att/*output*.txt"]) {
            no_sample = false;
            // check if the file is a symlink
            if sample.read_link().is_ok() {
                // check if the symlink is broken
                if sample.canonicalize().is_err() {
                    ui.send(UIMessage::Warning {
                        message: format!(
                            "Sample case {} is a broken link",
                            sample.strip_prefix(&task.path).unwrap().display()
                        ),
                    })?;
                }
            } else {
                ui.send(UIMessage::Warning {
                    message: format!(
                        "Sample case {} is not a symlink",
                        sample.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        }
        if no_sample {
            ui.send(UIMessage::Warning {
                message: "No sample file in att/".into(),
            })?;
        }
        Ok(())
    }
}

/// Check that the input files inside the att folder are valid, the solution doesn't crash with them
/// and the sample output files score full score.
#[derive(Debug, Default)]
pub struct AttSampleFilesValid;

impl SanityCheck<IOITask> for AttSampleFilesValid {
    fn name(&self) -> &'static str {
        "AttSampleFilesValid"
    }

    fn pre_hook(&mut self, task: &IOITask, eval: &mut EvaluationData) -> Result<(), Error> {
        let validator = &task.input_validator;
        let task_type = if let TaskType::Batch(data) = &task.task_type {
            data
        } else {
            return Ok(());
        };
        let official_solution = &task_type.output_generator;
        let samples = get_sample_files(task, eval)?;
        for (input, output) in samples {
            let input_name = input.strip_prefix(&task.path).unwrap().to_owned();
            let input_handle = File::new(format!("Sample input file at {}", input_name.display()));
            let input_uuid = input_handle.uuid;
            eval.dag.provide_file(input_handle, input)?;

            // validate the input file
            let (val_handle, val) = validator.validate(
                eval,
                format!("Validation of sample case {}", input_name.display()),
                0,
                0,
                input_uuid,
            )?;
            if let Some(val) = val {
                let input_name = input_name.clone();
                let sender = eval.sender.clone();
                eval.dag.on_execution_done(&val.uuid, move |res| {
                    if !res.status.is_success() {
                        sender.send(UIMessage::Warning {
                            message: format!(
                                "Sample input file {} is not valid",
                                input_name.display()
                            ),
                        })?;
                    }
                    Ok(())
                });
                eval.dag.add_execution(val);
            }

            if let Some(solution) = &official_solution {
                let output_name = output.strip_prefix(&task.path).unwrap().to_owned();
                let output_handle =
                    File::new(format!("Sample output file at {}", output_name.display()));
                let output_uuid = output_handle.uuid;
                eval.dag.provide_file(output_handle, output)?;

                // generate the output file
                let (correct_output, sol) = solution.generate(
                    task,
                    eval,
                    format!(
                        "Generation of output file relative to {}",
                        input_name.display()
                    ),
                    0,
                    0,
                    input_uuid,
                    val_handle,
                )?;
                let correct_output =
                    correct_output.ok_or_else(|| format_err!("Missing official solution"))?;
                if let Some(sol) = sol {
                    let sender = eval.sender.clone();
                    let output_name = output_name.clone();
                    eval.dag.on_execution_done(&sol.uuid, move |res| {
                        if !res.status.is_success() {
                            sender.send(UIMessage::Warning {
                                message: format!(
                                    "Solution failed on sample input file {}",
                                    output_name.display()
                                ),
                            })?;
                        }
                        Ok(())
                    });
                    eval.dag.add_execution(sol);
                }

                // validate the output with the correct one
                let sender = eval.sender.clone();
                let chk = task_type.checker.check(
                    eval,
                    0,
                    format!("Checking sample output {}", output_name.display()),
                    input_uuid,
                    correct_output,
                    output_uuid,
                    move |score, message| {
                        if abs_diff_ne!(score, 1.0) {
                            sender.send(UIMessage::Warning {
                                message: format!(
                                    "Sample output file {} scores {}: {}",
                                    output_name.display(),
                                    score,
                                    message
                                ),
                            })?;
                        }
                        Ok(())
                    },
                )?;
                eval.dag.add_execution(chk);
            }
        }
        Ok(())
    }
}

/// Search the input-output sample pairs inside the att folder. Returns a list of (input,output)
/// pairs with their numbers matching.
fn get_sample_files(
    task: &IOITask,
    eval: &mut EvaluationData,
) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
    let regex = Regex::new(r"(in|out)put(\d+)\.txt$").unwrap();
    let extract_num = |path: &PathBuf| {
        let path_str = path.to_string_lossy();
        let caps = regex.captures(path_str.as_ref());
        if let Some(caps) = caps {
            if let Some(num) = caps.get(2) {
                let num: TestcaseId = if let Ok(num) = num.as_str().parse() {
                    num
                } else {
                    return None;
                };
                return Some(num);
            }
        }
        None
    };
    let mut inputs = HashMap::new();
    for input in list_files(&task.path, vec!["att/*input*.txt"]) {
        if let Some(num) = extract_num(&input) {
            if let Some(i) = inputs.insert(num, input.clone()) {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "Duplicate sample input file with number {}: {} and {}",
                        num,
                        input.strip_prefix(&task.path).unwrap().display(),
                        i.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        }
    }
    let mut outputs = HashMap::new();
    for output in list_files(&task.path, vec!["att/*output*.txt"]) {
        if let Some(num) = extract_num(&output) {
            if let Some(o) = outputs.insert(num, output.clone()) {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "Duplicate sample output file with number {}: {} and {}",
                        num,
                        output.strip_prefix(&task.path).unwrap().display(),
                        o.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        }
    }
    let mut samples = Vec::new();
    for (num, input) in inputs {
        let output = if let Some(output) = outputs.remove(&num) {
            output
        } else {
            eval.sender.send(UIMessage::Warning {
                message: format!(
                    "Sample input file {} does not have its output file",
                    input.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
            continue;
        };
        samples.push((input, output));
    }
    for (_, output) in outputs {
        eval.sender.send(UIMessage::Warning {
            message: format!(
                "Sample output file {} does not have its input file",
                output.strip_prefix(&task.path).unwrap().display()
            ),
        })?;
    }
    Ok(samples)
}
