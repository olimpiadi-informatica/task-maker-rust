use failure::{bail, Error};

use task_maker_dag::File;

use crate::sanity_checks::SanityCheck;
use crate::terry::Task;
use crate::ui::UIMessage;
use crate::{list_files, EvaluationData, UISender, DATA_DIR};
use std::path::Path;

/// Fuzz the checker with some nasty output files making sure it doesn't crash.
///
/// The bad output files can be found inside `bad_outputs` in the data directory.
#[derive(Debug, Default)]
pub struct FuzzChecker;

impl SanityCheck<Task> for FuzzChecker {
    fn name(&self) -> &'static str {
        "FuzzChecker"
    }

    fn pre_hook(&mut self, task: &Task, eval: &mut EvaluationData) -> Result<(), Error> {
        let outputs_dir = DATA_DIR.join("bad_outputs");
        if !outputs_dir.exists() {
            bail!("DATA_DIR/bad_outputs does not exists");
        }
        let outputs = list_files(outputs_dir, vec!["*.txt"]);
        if outputs.is_empty() {
            return Ok(());
        }
        // keep the seed fixed for the cache
        let (input, gen) = task.generator.generate(
            eval,
            "Generation of input for FuzzChecker".into(),
            42,
            task.official_solution.clone(),
        )?;
        let sender = eval.sender.clone();
        eval.dag.on_execution_done(&gen.uuid, move |res| {
            if !res.status.is_success() {
                sender.send(UIMessage::Warning {
                    message: format!("Failed to generate input for FuzzChecker: {:?}", res.status),
                })?;
            }
            Ok(())
        });
        eval.dag.add_execution(gen);
        for output in outputs {
            let name = Path::new(output.file_name().expect("invalid file name"));
            let output_file = File::new(format!("Bad output {}", name.display()));
            let output_uuid = output_file.uuid;
            eval.dag.provide_file(output_file, &output)?;
            let sender = eval.sender.clone();
            let name2 = name.to_owned();
            let check = task.checker.check(
                eval,
                format!("Checking bad input {}", name.display()),
                input,
                output_uuid,
                task.official_solution.clone(),
                move |outcome| {
                    if let Err(e) = outcome {
                        sender.send(UIMessage::Warning {
                            message: format!(
                                "Checker failed on bad output {}: {}",
                                name2.display(),
                                e
                            ),
                        })?;
                    }
                    Ok(())
                },
            )?;
            let sender = eval.sender.clone();
            let name2 = name.to_owned();
            eval.dag.on_execution_done(&check.uuid, move |res| {
                if !res.status.is_success() {
                    sender.send(UIMessage::Warning {
                        message: format!(
                            "Checker failed on bad output {}: {:?}",
                            name2.display(),
                            res.status
                        ),
                    })?;
                }
                Ok(())
            });
            eval.dag.add_execution(check);
        }
        Ok(())
    }
}
