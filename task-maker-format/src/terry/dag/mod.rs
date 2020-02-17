use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, FileUuid};

use crate::terry::Seed;
use crate::{EvaluationData, SourceFile, Tag};

/// The source of the input files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputGenerator {
    /// The source file of the generator executable.
    source: Arc<SourceFile>,
}

/// The validator of the input files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputValidator {
    /// The source file of the validator executable.
    source: Arc<SourceFile>,
}

/// A solution to test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution;

/// The checker of the input/output files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checker {
    /// The source file of the checker executable.
    source: Arc<SourceFile>,
}

impl InputGenerator {
    /// Make a new `InputGenerator` based on the specified source file.
    pub fn new(source: Arc<SourceFile>) -> InputGenerator {
        InputGenerator { source }
    }

    /// Build the execution for the generation of the input file, but don't bind the execution
    /// callbacks.
    pub(crate) fn generate(
        &self,
        eval: &mut EvaluationData,
        description: String,
        seed: Seed,
        official_solution: Option<Arc<SourceFile>>,
    ) -> Result<(FileUuid, Execution), Error> {
        let mut exec =
            self.source
                .execute(eval, description, vec![seed.to_string(), "0".to_string()])?;
        include_official_solution(&mut exec, official_solution)?;
        exec.limits_mut().nproc = None;
        exec.tag(Tag::Generation.into());
        let input_file = exec.stdout();
        Ok((input_file.uuid, exec))
    }

    /// Build the execution for the generation of the input file, and bind the execution callbacks.
    pub(crate) fn generate_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        seed: Seed,
        official_solution: Option<Arc<SourceFile>>,
    ) -> Result<FileUuid, Error> {
        let (input, gen) = self.generate(
            eval,
            format!(
                "Generation of input file for {} with seed {}",
                solution, seed
            ),
            seed,
            official_solution,
        )?;
        if eval.dag.config_mut().copy_exe {
            eval.dag.write_file_to(
                input,
                eval.task_root.join(format!("bin/io/{}.in", solution)),
                false,
            );
        }
        // TODO: bind callbacks
        eval.dag.add_execution(gen);
        Ok(input)
    }
}

impl InputValidator {
    /// Make a new `InputValidator` based on the specified source file.
    pub fn new(source: Arc<SourceFile>) -> InputValidator {
        InputValidator { source }
    }

    /// Build the execution for the validation of the input file, but don't bind the execution
    /// callbacks.
    pub(crate) fn validate(
        &self,
        eval: &mut EvaluationData,
        description: String,
        input: FileUuid,
        official_solution: Option<Arc<SourceFile>>,
    ) -> Result<(FileUuid, Execution), Error> {
        let mut exec = self.source.execute(eval, description, Vec::<&str>::new())?;
        include_official_solution(&mut exec, official_solution)?;
        exec.limits_mut().nproc = None;
        exec.stdin(input).tag(Tag::Generation.into());
        let stdout = exec.stdout();
        Ok((stdout.uuid, exec))
    }

    /// Build the execution for the validation of the input file, and bind the execution callbacks.
    pub(crate) fn validate_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        input: FileUuid,
        official_solution: Option<Arc<SourceFile>>,
    ) -> Result<FileUuid, Error> {
        let (handle, val) = self.validate(
            eval,
            format!("Validation of input file for {}", solution),
            input,
            official_solution,
        )?;
        // TODO: bind callbacks
        eval.dag.add_execution(val);
        Ok(handle)
    }
}

impl Solution {
    /// Use the provided solution to generate an output file based on the provided input file. If
    /// the `validation_handle` is not `None`, the execution will wait for the validation to
    /// succeed.
    pub(crate) fn solve(
        eval: &mut EvaluationData,
        solution: &SourceFile,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<(FileUuid, Execution), Error> {
        let mut exec = solution.execute(
            eval,
            format!("Evaluation of solution {}", solution.name()),
            Vec::<&str>::new(),
        )?;
        exec.stdin(input);
        exec.tag(Tag::Evaluation.into());
        if let Some(validation) = validation_handle {
            exec.input(validation, "wait_for_validation", false);
        }
        let output = exec.stdout();
        // TODO set limits
        Ok((output.uuid, exec))
    }

    /// Same as `Solution::solve` but also binding the execution callbacks.
    pub(crate) fn solve_and_bind(
        eval: &mut EvaluationData,
        solution: &SourceFile,
        input: FileUuid,
        validation_handle: Option<FileUuid>,
    ) -> Result<FileUuid, Error> {
        let (output, sol) = Solution::solve(eval, solution, input, validation_handle)?;
        if eval.dag.config_mut().copy_exe {
            eval.dag.write_file_to(
                output,
                eval.task_root
                    .join(format!("bin/io/{}.out", solution.name())),
                false,
            );
        }
        // TODO bind callbacks
        eval.dag.add_execution(sol);
        Ok(output)
    }
}

impl Checker {
    /// Make a new `Checker` based on the specified source file.
    pub fn new(source: Arc<SourceFile>) -> Checker {
        Checker { source }
    }

    /// Build the execution for the checking of the output file of a solution.
    pub(crate) fn check(
        &self,
        eval: &mut EvaluationData,
        description: String,
        input: FileUuid,
        output: FileUuid,
        official_solution: Option<Arc<SourceFile>>,
        // TODO add callback
    ) -> Result<Execution, Error> {
        let mut exec = self
            .source
            .execute(eval, description, vec!["input.txt", "output.txt"])?;
        include_official_solution(&mut exec, official_solution)?;
        exec.limits_mut().nproc = None;
        exec.input(input, "input.txt", false)
            .input(output, "output.txt", false);
        let stdout = exec.stdout();
        let stderr = exec.stdout();
        eval.dag.urgent_file(&stdout);
        eval.dag.urgent_file(&stderr);
        // TODO process stdout/stderr and call callback
        Ok(exec)
    }

    /// Build the execution for the checking of the output file, and bind the execution callbacks.
    pub(crate) fn check_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        input: FileUuid,
        output: FileUuid,
        official_solution: Option<Arc<SourceFile>>,
        // TODO add callback
    ) -> Result<(), Error> {
        let exec = self.check(
            eval,
            format!("Checking output of {}", solution),
            input,
            output,
            official_solution,
        )?;
        // TODO bind callbacks
        eval.dag.add_execution(exec);
        Ok(())
    }
}

/// Include the compiled official solution to the sandbox of the provided execution.
fn include_official_solution(
    exec: &mut Execution,
    official_solution: Option<Arc<SourceFile>>,
) -> Result<(), Error> {
    if let Some(solution) = official_solution {
        exec.input(
            solution.executable()?,
            solution
                .write_bin_to()
                .expect("managers should always be copied")
                .file_name()
                .unwrap(),
            true,
        );
    }
    Ok(())
}
