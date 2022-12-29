use anyhow::{bail, Context, Error};

use task_maker_dag::File;

use crate::sanity_checks::{make_sanity_check, SanityCheck, SanityCheckCategory};
use crate::terry::TerryTask;
use crate::{list_files, EvaluationData, UISender, DATA_DIR};
use std::path::Path;
use task_maker_diagnostics::Diagnostic;

/// Fuzz the checker with some nasty output files making sure it doesn't crash.
///
/// The bad output files can be found inside `bad_outputs` in the data directory.
#[derive(Debug, Default)]
pub struct FuzzChecker;
make_sanity_check!(FuzzChecker);

impl SanityCheck<TerryTask> for FuzzChecker {
    fn name(&self) -> &'static str {
        "FuzzChecker"
    }

    fn category(&self) -> SanityCheckCategory {
        SanityCheckCategory::Checker
    }

    fn pre_hook(&self, task: &TerryTask, eval: &mut EvaluationData) -> Result<(), Error> {
        let outputs_dir = DATA_DIR.join("bad_outputs");
        if !outputs_dir.exists() {
            bail!("DATA_DIR/bad_outputs does not exists");
        }
        let outputs = list_files(outputs_dir, vec!["*.txt"]);
        if outputs.is_empty() {
            return Ok(());
        }
        // keep the seed fixed for the cache
        let seed = 42;
        let (input, mut gen) = task.generator.generate(
            eval,
            "Generation of input for FuzzChecker".into(),
            seed,
            task.official_solution.clone(),
        )?;
        gen.capture_stderr(1024);
        let sender = eval.sender.clone();
        eval.dag.on_execution_done(&gen.uuid, move |res| {
            if !res.status.is_success() {
                let mut diagnostic = Diagnostic::error(format!(
                    "Failed to generate input for FuzzChecker with seed {}",
                    seed
                ))
                .with_note(format!("The generator failed with: {:?}", res.status));
                if let Some(stderr) = res.stderr {
                    diagnostic = diagnostic
                        .with_help("The generator stderr is:")
                        .with_help_attachment(stderr);
                }
                sender.add_diagnostic(diagnostic)?;
            }
            Ok(())
        });
        eval.dag.add_execution(gen);
        for output in outputs {
            let name = Path::new(output.file_name().context("invalid file name")?);
            let output_file = File::new(format!("Bad output {}", name.display()));
            let output_uuid = output_file.uuid;
            eval.dag.provide_file(output_file, &output)?;
            let sender = eval.sender.clone();
            let name2 = name.to_owned();
            let mut check = task.checker.check(
                eval,
                format!("Checking bad input {}", name.display()),
                input,
                output_uuid,
                task.official_solution.clone(),
                move |outcome| {
                    if let Err(e) = outcome {
                        sender.add_diagnostic(Diagnostic::error(format!(
                            "Checker failed on bad output {}: {}",
                            name2.display(),
                            e
                        )))?;
                    }
                    Ok(())
                },
            )?;
            check.capture_stderr(1024);
            let sender = eval.sender.clone();
            let name2 = name.to_owned();
            eval.dag.on_execution_done(&check.uuid, move |res| {
                if !res.status.is_success() {
                    let mut diagnostic = Diagnostic::error(format!(
                        "Checker failed on bad output {}: {:?}",
                        name2.display(),
                        res.status
                    ))
                    .with_note(format!("The checker failed with: {:?}", res.status));
                    if let Some(stderr) = res.stderr {
                        diagnostic = diagnostic
                            .with_help("The checker's stderr is:")
                            .with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            });
            eval.dag.add_execution(check);
        }
        Ok(())
    }
}
