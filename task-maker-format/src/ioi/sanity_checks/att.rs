use failure::{format_err, Error};

use crate::ioi::sanity_checks::check_missing_graders;
use crate::ioi::Task;
use crate::ui::UIMessage;
use crate::{list_files, EvaluationData, UISender};

/// Check that all the graders are present inside att.
pub fn check_att_graders(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    check_missing_graders(task, eval, "att")
}

/// Check that all the templates are present inside att.
pub fn check_att_templates(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
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

/// Check that the sample cases inside att are valid symlinks.
pub fn check_att_sample_files(task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
    let mut no_sample = true;
    for sample in list_files(&task.path, vec!["att/*input*.txt", "att/*output*.txt"]) {
        no_sample = false;
        // check if the file is a symlink
        if sample.read_link().is_ok() {
            // check if the symlink is broken
            if sample.canonicalize().is_err() {
                eval.sender.send(UIMessage::Warning {
                    message: format!(
                        "Sample case {} is a broken link",
                        sample.strip_prefix(&task.path).unwrap().display()
                    ),
                })?;
            }
        } else {
            eval.sender.send(UIMessage::Warning {
                message: format!(
                    "Sample case {} is not a symlink",
                    sample.strip_prefix(&task.path).unwrap().display()
                ),
            })?;
        }
    }
    if no_sample {
        eval.sender.send(UIMessage::Warning {
            message: "No sample file in att/".into(),
        })?;
    }
    Ok(())
}
