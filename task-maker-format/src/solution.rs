use std::cmp::Ordering;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{bail, Context, Error};
use regex::Regex;
use serde::{Deserialize, Serialize};
use task_maker_diagnostics::{CodeSpan, Diagnostic};
use task_maker_lang::GraderMap;

use crate::{EvaluationData, SourceFile};

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
    pub fn new(
        path: &Path,
        base_dir: &Path,
        grader_map: Option<Arc<GraderMap>>,
        eval: &mut EvaluationData,
    ) -> Option<Self> {
        let write_to = base_dir
            .join("bin")
            .join("sol")
            .join(path.file_name().unwrap());
        let source_file = SourceFile::new(
            path,
            base_dir,
            format!("Solution at {}", path.display()),
            grader_map,
            Some(write_to),
        )?;
        Some(Self {
            source_file: Arc::new(source_file),
            checks: SolutionCheck::extract_check_list(path, eval).ok()?,
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
    /// Span of this check.
    pub code_span: CodeSpan,
}

impl SolutionCheck {
    /// Create a new [`SolutionCheck`] with the given result, that targets all the subtasks matching
    /// `pattern`.
    pub fn new(
        result: SolutionCheckResult,
        pattern: impl Into<String>,
        code_span: CodeSpan,
    ) -> Self {
        Self {
            result,
            subtask_name_pattern: pattern.into(),
            code_span,
        }
    }
}

/// Result of the evaluation of a solution on a testcase.
///
/// We define a partial order used to determine the correctness of solution checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TestcaseEvaluationResult {
    /// The solution scored 100% of the testcase.
    Accepted,
    /// The solution is partially correct.
    Partial,
    /// The output is wrong.
    WrongAnswer,
    /// The solution timed out.
    TimeLimitExceeded,
    /// The solution exceeded the wall time limit.
    WallTimeLimitExceeded,
    /// The solution exceeded the memory limit.
    MemoryLimitExceeded,
    /// The solution crashed.
    RuntimeError,
}

impl TestcaseEvaluationResult {
    fn proj(&self) -> i32 {
        match self {
            Self::Accepted => 2,
            Self::Partial => 1,
            Self::WrongAnswer => 0,
            Self::TimeLimitExceeded => 0,
            Self::WallTimeLimitExceeded => 0,
            Self::MemoryLimitExceeded => 0,
            Self::RuntimeError => 0,
        }
    }
}

impl PartialOrd for TestcaseEvaluationResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.proj(), other.proj()) {
            (a, b) if a > b => Some(Ordering::Greater),
            (a, b) if a < b => Some(Ordering::Less),
            (a, b) if a == b && self == other => Some(Ordering::Equal),
            _ => None,
        }
    }
}

/// The expected result of a solution in a set of subtasks.
///
/// Each `SolutionCheckResult` is described by:
/// * a long name ([`SolutionCheckResult::as_str`])
/// * a short name ([`SolutionCheckResult::as_compact_str`])
/// * a set of `TestcaseEvaluationResult` ([`SolutionCheckResult::minimals`])
///
/// We say that `outcomes: [TestcaseEvaluationResult]` satisfies a `sol_check: SolutionCheckResult` iff there
/// exists a minimal element `min` of `outcomes` such that `min` is contained in `sol_check.minimals()`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum SolutionCheckResult {
    /// The solution should get "Accepted" on all the testcases of the subtask.
    Accepted,
    /// The solution should get "Partial Score" on at least one testcase and "Accepted" on the others.
    PartialScore,
    /// The solution should get "Wrong Answer" on at least one testcase of the subtask.
    WrongAnswer,
    /// The solution should get "Time Limit Exceeded" on at least one testcase of the subtask.
    TimeLimitExceeded,
    /// The solution should get "Wallclock Time Limit Exceeded" on at least one testcase of the subtask.
    WallTimeLimitExceeded,
    /// The solution should get "Memory Limit Exceeded" on at least one testcase of the subtask.
    MemoryLimitExceeded,
    /// The solution should get "Runtime Error" on at least one testcase of the subtask.
    RuntimeError,
    /// The solution should get "WrongAnswer", "TimeLimitExceeded", "WallTimeLimitExceeded",
    /// "MemoryLimitExceeded" or "RuntimeError" on at least one testcase of the subtask.
    Zero,
}

impl FromStr for SolutionCheckResult {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "accepted" => Ok(Self::Accepted),
            "partial-score" => Ok(Self::PartialScore),
            "wrong-answer" => Ok(Self::WrongAnswer),
            "time-limit-exceeded" => Ok(Self::TimeLimitExceeded),
            "wall-time-limit-exceeded" => Ok(Self::WallTimeLimitExceeded),
            "memory-limit-exceeded" => Ok(Self::MemoryLimitExceeded),
            "runtime-error" => Ok(Self::RuntimeError),
            "zero" => Ok(Self::Zero),
            _ => bail!("Invalid check name: @check-{}", s),
        }
    }
}

impl SolutionCheckResult {
    /// List all [`SolutionCheckResult`] sorted by self.minimals().len().
    pub fn sorted_all() -> &'static [Self] {
        &[
            Self::Accepted,
            Self::PartialScore,
            Self::WrongAnswer,
            Self::TimeLimitExceeded,
            Self::WallTimeLimitExceeded,
            Self::MemoryLimitExceeded,
            Self::RuntimeError,
            Self::Zero,
        ]
    }

    /// Get the string representation of this [`SolutionCheckResult`], as used in @check rules.
    pub fn as_str(&self) -> &'static str {
        match self {
            SolutionCheckResult::Accepted => "accepted",
            SolutionCheckResult::PartialScore => "partial-score",
            SolutionCheckResult::WrongAnswer => "wrong-answer",
            SolutionCheckResult::TimeLimitExceeded => "time-limit-exceeded",
            SolutionCheckResult::WallTimeLimitExceeded => "wall-time-limit-exceeded",
            SolutionCheckResult::MemoryLimitExceeded => "memory-limit-exceeded",
            SolutionCheckResult::RuntimeError => "runtime-error",
            SolutionCheckResult::Zero => "zero",
        }
    }

    /// Get a compact representation of this result.
    ///
    /// For example `SolutionCheckResult::Accepted` is `"AC"`.
    pub fn as_compact_str(&self) -> &'static str {
        match self {
            SolutionCheckResult::Accepted => "AC",
            SolutionCheckResult::PartialScore => "PS",
            SolutionCheckResult::WrongAnswer => "WA",
            SolutionCheckResult::TimeLimitExceeded => "TLE",
            SolutionCheckResult::WallTimeLimitExceeded => "WTLE",
            SolutionCheckResult::MemoryLimitExceeded => "MLE",
            SolutionCheckResult::RuntimeError => "RE",
            SolutionCheckResult::Zero => "ZR",
        }
    }

    /// Check if this result is valid with respect to the actual outcomes.
    pub fn check(&self, outcomes: &[TestcaseEvaluationResult]) -> bool {
        for outcome in outcomes {
            if outcomes.iter().any(|o| o < outcome) {
                continue;
            }

            if self.minimals().contains(outcome) {
                return true;
            }
        }

        if outcomes.is_empty()
            && self
                .minimals()
                .contains(&TestcaseEvaluationResult::Accepted)
        {
            return true;
        }

        false
    }

    /// Return the set of matching minimal results.
    pub fn minimals(&self) -> &'static [TestcaseEvaluationResult] {
        use TestcaseEvaluationResult as TER;
        match self {
            Self::Accepted => &[TER::Accepted],
            Self::PartialScore => &[TER::Partial],
            Self::WrongAnswer => &[TER::WrongAnswer],
            Self::TimeLimitExceeded => &[TER::TimeLimitExceeded],
            Self::WallTimeLimitExceeded => &[TER::WallTimeLimitExceeded],
            Self::MemoryLimitExceeded => &[TER::MemoryLimitExceeded],
            Self::RuntimeError => &[TER::RuntimeError],
            Self::Zero => &[
                TER::WrongAnswer,
                TER::TimeLimitExceeded,
                TER::WallTimeLimitExceeded,
                TER::MemoryLimitExceeded,
                TER::RuntimeError,
            ],
        }
    }
}

impl SolutionCheck {
    /// Try to extract the list of [`SolutionCheck`] from a file.
    pub fn extract_check_list<P: AsRef<Path>>(
        path: P,
        eval: &mut EvaluationData,
    ) -> Result<Vec<Self>, Error> {
        lazy_static! {
            static ref FIND_CHECKS: Regex = Regex::new(r".*(@check-.*)").expect("Invalid regex");
            static ref EXTRACT_CHECKS: Regex = Regex::new(
                r"(?x)
            @check-     # signal the start of a check
            (?P<result>accepted|partial-score|wrong-answer|time-limit-exceeded|memory-limit-exceeded|runtime-error|zero)
            :
            (?P<subtasks>
              (?:
                \s*     # spaces between subtask names
                [^\s]+  # subtask name
              )*        # allow a check without any subtask listed
            )
            \s*         # ignore spaces after the last subtask
        ")
            .expect("Invalid regex");
        }

        let path = path.as_ref();
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let mut checks = vec![];
        let mut file_offset = 0;
        for line in content.split('\n') {
            file_offset += line.len() + 1; // Includes the \n.
            let found = match FIND_CHECKS.captures(line).and_then(|c| c.get(1)) {
                None => continue,
                Some(found) => found,
            };
            let captures = EXTRACT_CHECKS.captures_iter(line).next();
            let path = path.strip_prefix(&eval.task_root).unwrap_or(path);
            // file_offset includes the current line length.
            let offset = file_offset - 1 - line.len() + found.start();

            if let Some(captures) = captures {
                let capture = captures.get(0).unwrap();
                let len = capture.end() - capture.start();
                let code_span = CodeSpan::from_str(path, &content, offset, len)
                    .context("Failed to build CodeSpan for check rule")?;
                let result = &captures["result"];
                let result = SolutionCheckResult::from_str(result)?;
                let patterns = &captures["subtasks"];
                for pattern in split_patterns(patterns) {
                    checks.push(Self::new(result, pattern, code_span.clone()));
                }
            } else {
                let len = found.end() - found.start();
                let mut diagnostic = Diagnostic::error(format!(
                    "In '{}' the check '{}' is not valid",
                    path.display(),
                    line
                ));
                if let Ok(span) = CodeSpan::from_str(path, &content, offset, len) {
                    diagnostic = diagnostic.with_code_span(span);
                }
                let _ = eval.add_diagnostic(diagnostic);
            }
        }
        Ok(checks)
    }
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

    use crate::solution::{SolutionCheck, SolutionCheckResult};
    use crate::EvaluationData;

    fn get_checks(source: &str) -> Result<Vec<SolutionCheck>, Error> {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let path = tmpdir.path().join("source.txt");
        std::fs::write(&path, source).unwrap();
        let mut eval = EvaluationData::new(tmpdir.path()).0;
        SolutionCheck::extract_check_list(path, &mut eval)
    }

    #[test]
    fn test_extract_check_list() {
        let checks = get_checks(
            r"
           /*
            * @check-accepted: st1 st2 st3*
            * @check-partial-score: asd
            * @check-wrong-answer: asd
            * @check-wrong-answer:
            * @check-time-limit-exceeded: asd
            * @check-memory-limit-exceeded: asd
            * @check-runtime-error: asd
            * @check-zero: asd
            */
        ",
        )
        .unwrap();
        assert_eq!(checks[0].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[0].subtask_name_pattern, "st1");
        assert_eq!(
            checks[0].code_span.as_str(),
            "@check-accepted: st1 st2 st3*"
        );
        assert_eq!(checks[1].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[1].subtask_name_pattern, "st2");
        assert_eq!(
            checks[1].code_span.as_str(),
            "@check-accepted: st1 st2 st3*"
        );
        assert_eq!(checks[2].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[2].subtask_name_pattern, "st3*");
        assert_eq!(
            checks[2].code_span.as_str(),
            "@check-accepted: st1 st2 st3*"
        );
        assert_eq!(checks[3].result, SolutionCheckResult::PartialScore);
        assert_eq!(checks[3].subtask_name_pattern, "asd");
        assert_eq!(checks[3].code_span.as_str(), "@check-partial-score: asd");
        assert_eq!(checks[4].result, SolutionCheckResult::WrongAnswer);
        assert_eq!(checks[4].subtask_name_pattern, "asd");
        assert_eq!(checks[4].code_span.as_str(), "@check-wrong-answer: asd");
        assert_eq!(checks[5].result, SolutionCheckResult::TimeLimitExceeded);
        assert_eq!(checks[5].subtask_name_pattern, "asd");
        assert_eq!(
            checks[5].code_span.as_str(),
            "@check-time-limit-exceeded: asd"
        );
        assert_eq!(checks[6].result, SolutionCheckResult::MemoryLimitExceeded);
        assert_eq!(checks[6].subtask_name_pattern, "asd");
        assert_eq!(
            checks[6].code_span.as_str(),
            "@check-memory-limit-exceeded: asd"
        );
        assert_eq!(checks[7].result, SolutionCheckResult::RuntimeError);
        assert_eq!(checks[7].subtask_name_pattern, "asd");
        assert_eq!(checks[7].code_span.as_str(), "@check-runtime-error: asd");
        assert_eq!(checks[8].result, SolutionCheckResult::Zero);
        assert_eq!(checks[8].subtask_name_pattern, "asd");
        assert_eq!(checks[8].code_span.as_str(), "@check-zero: asd");
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
            &"
           /*
            * @check-accepted: \tst1 \t\u{000B}\u{000C}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{200A} st2\t  \t<space><space><space>
            */
        ".replace("<space>", " "),
        )
        .unwrap();
        assert_eq!(checks[0].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[0].subtask_name_pattern, "st1");
        assert_eq!(checks[0].code_span.as_str(), "@check-accepted: \tst1 \t\u{000B}\u{000C}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{200A} st2\t  \t   ");
        assert_eq!(checks[1].result, SolutionCheckResult::Accepted);
        assert_eq!(checks[1].subtask_name_pattern, "st2");
        assert_eq!(checks[1].code_span.as_str(), "@check-accepted: \tst1 \t\u{000B}\u{000C}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{200A} st2\t  \t   ");
    }
}
