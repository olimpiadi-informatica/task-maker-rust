use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Error};
use regex::Regex;
use serde::{Deserialize, Serialize};

use task_maker_lang::GraderMap;

use crate::SourceFile;

/// A solution to evaluate. This includes the source file and some additional metadata.
#[derive(Clone, Debug)]
pub struct Solution {
    /// A reference to the source file of this solution.
    pub source_file: Arc<SourceFile>,
    /// The set of checks to perform on the solution.
    pub checks: Vec<SolutionCheck>,
}

impl Solution {
    /// Create a new [`Solution`] for a given source file.
    ///
    /// Returns `None` if the language is unknown.
    pub fn new(path: &Path, base_dir: &Path, grader_map: Option<Arc<GraderMap>>) -> Option<Self> {
        let write_to = base_dir
            .join("bin")
            .join("sol")
            .join(path.file_name().unwrap());
        let source_file = SourceFile::new(path, base_dir, grader_map, Some(write_to))?;
        Some(Self {
            source_file: Arc::new(source_file),
            checks: extract_check_list(path).ok()?,
        })
    }
}

/// Some information about a solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionInfo {
    /// The path on disk of this solution.
    pub path: PathBuf,
    /// The name of this solution.
    pub name: String,
    /// The name of the language of this solution.
    pub language_name: String,
    /// The list of checks specified inside the source file.
    pub checks: Vec<SolutionCheck>,
}

impl From<&Solution> for SolutionInfo {
    fn from(solution: &Solution) -> Self {
        Self {
            path: solution.source_file.path.clone(),
            name: solution.source_file.name(),
            language_name: solution.source_file.language().name().into(),
            checks: solution.checks.clone(),
        }
    }
}

/// A check to perform on a solution, against a subtask.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolutionCheck {
    /// The expected result of the solution.
    pub result: SolutionCheckResult,
    /// The pattern that should match the name of the subtask to check.
    pub subtask_name_pattern: String,
}

impl SolutionCheck {
    pub fn new(result: SolutionCheckResult, pattern: impl Into<String>) -> Self {
        Self {
            result,
            subtask_name_pattern: pattern.into(),
        }
    }
}

/// The expected result of a solution in a set of subtasks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum SolutionCheckResult {
    /// The solution should get "Accepted" on all the testcases of the subtask.
    Accepted,
    /// The solution should get "Wrong Answer" on at least one testcase of the subtask.
    WrongAnswer,
    /// The solution should get "Time Limit Exceeded" on at least one testcase of the subtask.
    TimeLimitExceeded,
    /// The solution should get "Memory Limit Exceeded" on at least one testcase of the subtask.
    MemoryLimitExceeded,
    /// The solution should get "Runtime Error" on at least one testcase of the subtask.
    RuntimeError,
}

impl SolutionCheckResult {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "accepted" => Some(Self::Accepted),
            "wrong-answer" => Some(Self::WrongAnswer),
            "time-limit-exceeded" => Some(Self::TimeLimitExceeded),
            "memory-limit-exceeded" => Some(Self::MemoryLimitExceeded),
            "runtime-error" => Some(Self::RuntimeError),
            _ => None,
        }
    }
}

fn extract_check_list<P: AsRef<Path>>(path: P) -> Result<Vec<SolutionCheck>, Error> {
    lazy_static! {
        static ref FIND_CHECKS: Regex = Regex::new(r".*@check-.*").expect("Invalid regex");
        static ref EXTRACT_CHECKS: Regex = Regex::new(
            r"(?x)
            @check-      # signal the start of a check
            (?P<result>accepted|wrong-answer|time-limit-exceeded|memory-limit-exceeded|runtime-error)
            :
            (?P<subtasks>
              (?:
                \s*  # spaces between subtask names
                [^\s]+  # subtask name
              )*        # allow a check without any subtask listed
            )
            \s*      # ignore spaces after the last subtask
        ")
        .expect("Invalid regex");
    }

    let path = path.as_ref();
    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let mut checks = vec![];
    for line in content.lines() {
        if !FIND_CHECKS.is_match(line) {
            continue;
        }
        let captures = EXTRACT_CHECKS.captures_iter(line).next();
        if let Some(captures) = captures {
            let result = &captures["result"];
            let result = SolutionCheckResult::from_str(result)
                .ok_or_else(|| anyhow!("Invalid check result: {}", result))?;
            let patterns = &captures["subtasks"];
            for pattern in split_patterns(patterns) {
                checks.push(SolutionCheck::new(result, pattern));
            }
        } else {
            // FIXME: the check is invalid! Emit a proper message
            eprintln!("Invalid check: {:?}", line);
        }
    }
    Ok(checks)
}

/// Split the patterns by whitespace.
fn split_patterns(patterns: &str) -> Vec<&str> {
    let mut result = vec![];
    for piece in patterns.split_whitespace() {
        if !piece.is_empty() {
            result.push(piece);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::solution::{extract_check_list, SolutionCheck, SolutionCheckResult};

    fn get_checks(source: &str) -> Result<Vec<SolutionCheck>, Error> {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("source.txt");
        std::fs::write(&path, source).unwrap();
        extract_check_list(path)
    }

    #[test]
    fn test_extract_check_list() {
        let checks = get_checks(
            r"
           /*
            * @check-accepted: st1 st2 st3*
            * @check-wrong-answer: asd
            * @check-wrong-answer:
            * @check-time-limit-exceeded: asd
            * @check-memory-limit-exceeded: asd
            * @check-runtime-error: asd
            */
        ",
        )
        .unwrap();
        assert_eq!(checks[0].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[0].subtask_name_pattern, "st1");
        assert_eq!(checks[1].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[1].subtask_name_pattern, "st2");
        assert_eq!(checks[2].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[2].subtask_name_pattern, "st3*");
        assert_eq!(checks[3].result, SolutionCheckResult::WrongAnswer);
        assert_eq!(checks[3].subtask_name_pattern, "asd");
        assert_eq!(checks[4].result, SolutionCheckResult::TimeLimitExceeded);
        assert_eq!(checks[4].subtask_name_pattern, "asd");
        assert_eq!(checks[5].result, SolutionCheckResult::MemoryLimitExceeded);
        assert_eq!(checks[5].subtask_name_pattern, "asd");
        assert_eq!(checks[6].result, SolutionCheckResult::RuntimeError);
        assert_eq!(checks[6].subtask_name_pattern, "asd");
    }

    #[test]
    fn test_extract_check_list_invalid_name() {
        let checks = get_checks(
            r"
           /*
            * @check-lolnope: st1
            * @check
            */
        ",
        )
        .unwrap();
        assert!(checks.is_empty());
    }

    #[test]
    fn test_extract_check_list_spaces() {
        let checks = get_checks(
            "
           /*
            * @check-accepted: \tst1 \t\u{000B}\u{000C}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{200A} st2\t  \t   
            */
        ",
        )
        .unwrap();
        assert_eq!(checks[0].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[0].subtask_name_pattern, "st1");
        assert_eq!(checks[1].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[1].subtask_name_pattern, "st2");
    }
}
