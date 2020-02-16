use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Serialize};

use task_maker_dag::{Execution, FileUuid};

use crate::terry::Seed;
use crate::{EvaluationData, SourceFile};

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
    ) -> Result<(FileUuid, Option<Execution>), Error> {
        unimplemented!()
    }

    /// Build the execution for the generation of the input file, and bind the execution callbacks.
    pub(crate) fn generate_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        seed: Seed,
    ) -> Result<FileUuid, Error> {
        unimplemented!()
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
    ) -> Result<(FileUuid, Option<Execution>), Error> {
        unimplemented!()
    }

    /// Build the execution for the validation of the input file, and bind the execution callbacks.
    pub(crate) fn validate_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        input: FileUuid,
    ) -> Result<FileUuid, Error> {
        unimplemented!()
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
        // TODO add callback
    ) -> Result<(), Error> {
        unimplemented!()
    }

    /// Build the execution for the checking of the output file, and bind the execution callbacks.
    pub(crate) fn check_and_bind(
        &self,
        eval: &mut EvaluationData,
        solution: String,
        input: FileUuid,
        output: FileUuid,
        // TODO add callback
    ) -> Result<(), Error> {
        unimplemented!()
    }
}
