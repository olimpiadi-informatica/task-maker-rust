use std::collections::HashMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use failure::_core::fmt::Formatter;
use failure::{bail, format_err, Error};
use pest::Parser;

use crate::ioi::format::italian_yaml::TaskInputEntry;
use crate::ioi::{
    InputGenerator, InputValidator, OutputGenerator, SubtaskId, SubtaskInfo, TestcaseId,
    TestcaseInfo,
};
use crate::SourceFile;

/// This module exists because of a `pest`'s bug: https://github.com/pest-parser/pest/issues/326
#[allow(missing_docs)]
mod parser {
    /// The gen/cases.gen file parser.
    #[derive(Parser)]
    #[grammar = "ioi/format/italian_yaml/cases.gen.pest"]
    pub struct CasesGenParser;
}

/// Helper type for lightening the types.
type Pair<'a> = pest::iterators::Pair<'a, parser::Rule>;

/// A manager is either a generator or a validator, since they have the same internal structure they
/// are abstracted as a `Manager`.
#[derive(Debug)]
struct Manager {
    /// Source file of the manager.
    source: Arc<SourceFile>,
    /// Symbolic arguments to pass to the manager. All the variables will be replaced with their
    /// value.
    args: Vec<String>,
}

/// Operand of a constraint. It is either a constant integer value or a symbolic variable to
/// substitute.
#[derive(Debug)]
enum ConstraintOperand {
    /// This operand is a constant integer value.
    Constant(i64),
    /// This operand is a symbolic variable. It is stored the variable name, without the dollar
    /// sign.
    Variable(String),
}

/// The operator of a constraint.
#[derive(Debug)]
enum ConstraintOperator {
    /// Operator `<`.
    Less,
    /// Operator `<=`.
    LessEqual,
    /// Operator `>`.
    Greater,
    /// Operator `>=`.
    GreaterEqual,
    /// Operator `=`.
    Equal,
}

/// A constraint between the variables. It is in the following format:
///     operand (operator operand)+
/// Note that the number of operands is one more than the operators.
/// All the operators must be _in the same direction_: in the same constraint there cannot be both
/// a _less_ operator and a _greater_ one.
#[derive(Default)]
struct Constraint {
    /// List of the operands of the constraint.
    operands: Vec<ConstraintOperand>,
    /// List of the operators of the contraint.
    operators: Vec<ConstraintOperator>,
}

/// Temporary structure with the metadata of the parsing of the `cases.gen` file. The internal data
/// is filled and updated during the parsing of the file.
#[derive(Derivative)]
#[derivative(Debug)]
struct CasesGen<O>
where
    O: Fn(TestcaseId) -> OutputGenerator,
{
    /// The base directory of the task.
    task_dir: PathBuf,
    /// The function to call for getting the `OutputGenerator` for a given testcase.
    #[derivative(Debug = "ignore")]
    get_output_gen: O,
    /// The resulting `TaskInputEntry` that will be produced after the parsing of the `cases.gen`
    /// file.
    result: Vec<TaskInputEntry>,
    /// The list of constraints found in the file.
    constraints: Vec<Constraint>,
    /// The list of all the generators found, indexed by generator name.
    generators: HashMap<String, Manager>,
    /// The list of all the validators found, indexed by validator name.
    validators: HashMap<String, Manager>,
    /// The name of the default generator of the task. It's the generator with name `default`, if
    /// present. Each subtask will use this generator, unless specified.
    default_generator: Option<String>,
    /// The name of the default validator of the task. It's the validator with name `default`, if
    /// present. Each subtask will use this validator, unless specified.
    default_validator: Option<String>,
    /// The current generator for this subtask, it's the task's default, unless specified.
    current_generator: Option<String>,
    /// The current validator for this subtask, it's the task's default, unless specified.
    current_validator: Option<String>,
    /// The identifier of the next subtask to process.
    subtask_id: SubtaskId,
    /// The identifier of the next testcase to process.
    testcase_id: TestcaseId,
}

/// Parse the `gen/cases.gen` file extracting the subtasks and the testcases.
pub(crate) fn parse_cases_gen<P: AsRef<Path>, O>(
    path: P,
    output_gen: O,
) -> Result<Box<dyn Iterator<Item = TaskInputEntry>>, Error>
where
    O: Fn(TestcaseId) -> OutputGenerator,
{
    Ok(Box::new(
        CasesGen::new(path, output_gen)?.result.into_iter(),
    ))
}

impl<O> CasesGen<O>
where
    O: Fn(TestcaseId) -> OutputGenerator,
{
    /// Parse the `cases.gen` file pointed at the specified path.
    fn new<P: AsRef<Path>>(path: P, output_gen: O) -> Result<CasesGen<O>, Error> {
        let task_dir = path
            .as_ref()
            .parent()
            .expect("Invalid gen/cases.gen path")
            .parent()
            .expect("Invalid gen/cases.gen path");
        let content = std::fs::read_to_string(&path)?;
        let mut file = parser::CasesGenParser::parse(parser::Rule::file, &content)?;
        let file = file.next().ok_or_else(|| format_err!("Corrupted parser"))?; // extract the real file

        let mut cases = CasesGen {
            task_dir: task_dir.into(),
            get_output_gen: output_gen,
            result: vec![],
            constraints: vec![],
            generators: Default::default(),
            validators: Default::default(),
            default_generator: None,
            default_validator: None,
            current_generator: None,
            current_validator: None,
            subtask_id: 0,
            testcase_id: 0,
        };

        for line in file.into_inner() {
            match line.as_rule() {
                parser::Rule::line => {
                    let line = line
                        .into_inner()
                        .next()
                        .ok_or_else(|| format_err!("Corrupted parser"))?;
                    match line.as_rule() {
                        parser::Rule::command => {
                            let command = line
                                .into_inner()
                                .next()
                                .ok_or_else(|| format_err!("Corrupted parser"))?;
                            cases.parse_command(command)?;
                        }
                        parser::Rule::testcase => {
                            cases.parse_testcase(line.as_str())?;
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
        Ok(cases)
    }

    /// Parse a line with a command: one of the `:` prefixed actions.
    fn parse_command(&mut self, line: Pair) -> Result<(), Error> {
        match line.as_rule() {
            parser::Rule::GEN => {
                self.parse_gen(line)?;
            }
            parser::Rule::VAL => {
                self.parse_val(line)?;
            }
            parser::Rule::CONSTRAINT => {
                self.parse_constraint(line)?;
            }
            parser::Rule::SUBTASK => {
                self.parse_subtask(line)?;
            }
            parser::Rule::COPY => {
                self.parse_copy(line)?;
            }
            parser::Rule::RUN => {
                self.parse_run(line)?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Parse a raw testcase, a line not starting with `:`.
    fn parse_testcase(&mut self, line: &str) -> Result<(), Error> {
        if self.current_generator.is_none() {
            bail!("Cannot generate testcase: no default generator set");
        }
        let args = shell_words::split(line)
            .map_err(|e| format_err!("Invalid command arguments for testcase '{}': {}", line, e))?;
        let generator = InputGenerator::Custom(
            self.generators[self.current_generator.as_ref().unwrap()]
                .source
                .clone(),
            args,
        );
        // TODO check arguments
        self.result.push(TaskInputEntry::Testcase(TestcaseInfo {
            id: self.testcase_id,
            input_generator: generator,
            input_validator: self.get_validator(),
            output_generator: (self.get_output_gen)(self.testcase_id),
        }));
        Ok(())
    }

    /// Parse a `GEN` or a `VAL`, since they have the same internal format their parsing function is
    /// abstracted in this.
    fn process_gen_val(
        line: Vec<Pair>,
        task_dir: &Path,
        default: &mut Option<String>,
        current: &mut Option<String>,
        managers: &mut HashMap<String, Manager>,
        kind: &str,
    ) -> Result<(), Error> {
        let name = line[0].as_str();
        // Case 1: GEN|VAL name
        // Set the current generator/validator to the specified one
        if line.len() == 1 {
            if !managers.contains_key(name) {
                bail!(
                    "Cannot set the current {} to '{}': unknown {}",
                    kind,
                    name,
                    kind
                );
            }
            *current = Some(name.to_string());
        // Case 2: GEN|VAL name path [args...]
        // Add a new generator/validator to the list
        } else {
            let path = line[1].as_str();
            let path = task_dir.join(path);
            if !path.exists() {
                bail!(
                    "Cannot add {} '{}': '{}' does not exists",
                    kind,
                    name,
                    path.display()
                );
            }
            let source = SourceFile::new(
                &path,
                task_dir,
                None,
                Some(
                    task_dir
                        .join("bin")
                        .join(path.file_name().expect("invalid file name")),
                ),
            )
            .map(Arc::new)
            .ok_or_else(|| {
                format_err!("Cannot use {} '{}': unknown language", kind, path.display())
            })?;
            let args = shell_words::split(line[2].as_str())
                .map_err(|e| format_err!("Invalid arguments of '{}': {}", name, e))?;
            managers.insert(name.to_string(), Manager { source, args });
            if default.is_none() || name == "default" {
                *default = Some(name.to_string());
            }
        }
        Ok(())
    }

    /// Parse a `:GEN` command.
    fn parse_gen(&mut self, line: Pair) -> Result<(), Error> {
        let line: Vec<_> = line.into_inner().collect();
        CasesGen::<O>::process_gen_val(
            line,
            &self.task_dir,
            &mut self.default_generator,
            &mut self.current_generator,
            &mut self.generators,
            "generator",
        )?;
        Ok(())
    }

    /// Parse a `:VAL` command.
    fn parse_val(&mut self, line: Pair) -> Result<(), Error> {
        let line: Vec<_> = line.into_inner().collect();
        CasesGen::<O>::process_gen_val(
            line,
            &self.task_dir,
            &mut self.default_validator,
            &mut self.current_validator,
            &mut self.validators,
            "validator",
        )?;
        Ok(())
    }

    /// Parse a `:CONSTRAINT` command.
    fn parse_constraint(&mut self, line: Pair) -> Result<(), Error> {
        // TODO add support for subtask-level constraints
        let line_str = line.as_str().to_string();
        let line: Vec<_> = line.into_inner().collect();
        let mut constraint = Constraint::default();
        let mut direction = None;
        for item in line {
            match item.as_rule() {
                parser::Rule::number => {
                    constraint.operands.push(ConstraintOperand::Constant(
                        i64::from_str(item.as_str()).map_err(|e| {
                            format_err!(
                                "Invalid integer constant '{}' in constraint '{}': {}",
                                item.as_str(),
                                line_str,
                                e
                            )
                        })?,
                    ));
                }
                parser::Rule::variable => {
                    constraint
                        .operands
                        .push(ConstraintOperand::Variable(item.as_str()[1..].to_string()));
                }
                parser::Rule::comp_operator => {
                    let operator = ConstraintOperator::from_str(item.as_str())?;
                    let dir = match operator {
                        ConstraintOperator::Less | ConstraintOperator::LessEqual => Some(true),
                        ConstraintOperator::Greater | ConstraintOperator::GreaterEqual => {
                            Some(false)
                        }
                        ConstraintOperator::Equal => None,
                    };
                    if direction.is_none() {
                        direction = dir;
                    }
                    if dir.is_some() && direction != dir {
                        bail!("Malformed constraint: inequality direction must be the same")
                    }
                    constraint.operators.push(operator)
                }
                _ => unreachable!(),
            }
        }
        if constraint.operators.len() + 1 != constraint.operands.len() {
            bail!("Malformed constraint: invalid number of operators");
        }
        if constraint.operands.len() < 2 {
            bail!("Malformed constraint: too few operands");
        }
        self.constraints.push(constraint);
        Ok(())
    }

    /// Parse a `:SUBTASK` command.
    fn parse_subtask(&mut self, line: Pair) -> Result<(), Error> {
        let line: Vec<_> = line.into_inner().collect();
        self.current_generator = self.default_generator.clone();
        self.current_validator = self.default_validator.clone();
        let score = f64::from_str(line[0].as_str()).map_err(|e| {
            format_err!(
                "Invalid subtask score for subtask {}: {}",
                self.subtask_id,
                e
            )
        })?;
        let description = if line.len() >= 2 {
            Some(line[1].as_str().to_string())
        } else {
            None
        };
        self.result.push(TaskInputEntry::Subtask(SubtaskInfo {
            id: self.subtask_id,
            description,
            max_score: score,
            testcases: HashMap::new(),
        }));
        self.subtask_id += 1;
        Ok(())
    }

    /// Parse a `:COPY` command.
    fn parse_copy(&mut self, line: Pair) -> Result<(), Error> {
        let path = line.into_inner().next().expect("corrupted parser").as_str();
        let path = self.task_dir.join(path);
        if !path.exists() {
            bail!(
                "Cannot copy testcase from '{}': file not found",
                path.display()
            );
        }
        self.result.push(TaskInputEntry::Testcase(TestcaseInfo {
            id: self.testcase_id,
            input_generator: InputGenerator::StaticFile(path),
            input_validator: self.get_validator(),
            output_generator: (self.get_output_gen)(self.testcase_id),
        }));
        self.testcase_id += 1;
        Ok(())
    }

    /// Get the current validator for the next testcase.
    fn get_validator(&self) -> InputValidator {
        match &self.current_validator {
            // TODO: build the command line argument
            Some(val) => InputValidator::Custom(
                self.validators[val].source.clone(),
                vec![
                    "tm_validation_file".to_string(),
                    (self.subtask_id + 1).to_string(),
                ],
            ),
            None => InputValidator::AssumeValid,
        }
    }

    /// Parse a `:RUN` command.
    fn parse_run(&mut self, line: Pair) -> Result<(), Error> {
        let line_str = line.as_str().to_string();
        let line: Vec<_> = line.into_inner().collect();
        let name = line[0].as_str();
        let args = shell_words::split(line[1].as_str()).map_err(|e| {
            format_err!(
                "Invalid command arguments for RUN command '{}': {}",
                line_str,
                e
            )
        })?;
        if !self.generators.contains_key(name) {
            bail!("Generator '{}' not declared", name);
        }
        let generator = InputGenerator::Custom(self.generators[name].source.clone(), args);
        // TODO: check arguments
        self.result.push(TaskInputEntry::Testcase(TestcaseInfo {
            id: self.testcase_id,
            input_generator: generator,
            input_validator: self.get_validator(),
            output_generator: (self.get_output_gen)(self.testcase_id),
        }));
        self.testcase_id += 1;
        Ok(())
    }
}

impl FromStr for ConstraintOperator {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "<" => Ok(ConstraintOperator::Less),
            "<=" => Ok(ConstraintOperator::LessEqual),
            ">" => Ok(ConstraintOperator::Greater),
            ">=" => Ok(ConstraintOperator::GreaterEqual),
            "=" => Ok(ConstraintOperator::Equal),
            _ => bail!("Invalid operator: {}", s),
        }
    }
}

impl ToString for ConstraintOperator {
    fn to_string(&self) -> String {
        match self {
            ConstraintOperator::Less => "<",
            ConstraintOperator::LessEqual => "<=",
            ConstraintOperator::Greater => ">",
            ConstraintOperator::GreaterEqual => ">=",
            ConstraintOperator::Equal => "=",
        }
        .to_string()
    }
}

impl ToString for ConstraintOperand {
    fn to_string(&self) -> String {
        match self {
            ConstraintOperand::Constant(k) => k.to_string(),
            ConstraintOperand::Variable(v) => format!("${}", v),
        }
    }
}

impl Debug for Constraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut constraint = self.operands[0].to_string();
        for (op, var) in self.operators.iter().zip(self.operands[1..].iter()) {
            constraint += &format!(" {} {}", op.to_string(), var.to_string());
        }
        write!(f, "{}", constraint)
    }
}

#[cfg(test)]
mod tests {
    use crate::ioi::format::italian_yaml::cases_gen::CasesGen;
    use crate::ioi::OutputGenerator;

    #[test]
    fn test() {
        let res = CasesGen::new(
            "/home/edomora97/Workbench/olimpiadi/oii/problemi/incendio/gen/cases.gen",
            |_| OutputGenerator::StaticFile("nope".into()),
        )
        .unwrap();
        eprintln!("{:#?}", res);
    }
}
