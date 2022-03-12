use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::{bail, Error};
use colored::Colorize;

use crate::DiagnosticLevel;

/// A [`CodeSpan`] represent a slice of code.
///
/// At the moment the slice must not span on multiple lines.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct CodeSpan {
    /// The path of the file where this span comes from.
    file_name: PathBuf,
    /// The line number inside the file.
    line_number: NonZeroUsize,
    /// The offset of the first byte of the span, relative to the start of the file.
    file_offset: usize,
    /// The offset of the first byte of the span, relative to the start of the line.
    line_offset: usize,
    /// The length of the span.
    len: usize,
    /// The content of the line.
    line: String,
}

impl CodeSpan {
    /// Create a new [`CodeSpan`] from the content of a file, and the start-length pair.
    pub fn from_str(
        file_name: impl Into<PathBuf>,
        content: impl AsRef<str>,
        offset: usize,
        len: usize,
    ) -> Result<Self, Error> {
        let mut previous_lines_len = 0;
        let mut skipped_lines = 0;
        for line in content.as_ref().split('\n') {
            if previous_lines_len + line.len() == offset {
                bail!("Offset cannot be on the newline character");
            }
            if previous_lines_len + line.len() < offset {
                previous_lines_len += line.len() + 1; // Includes \n.
                skipped_lines += 1;
                continue;
            }

            let line_offset = offset - previous_lines_len;
            if line_offset + len > line.len() {
                bail!("Multiline spans are not supported");
            }
            assert!(line_offset + len <= line.len());
            return Ok(Self {
                file_name: file_name.into(),
                line_number: NonZeroUsize::new(skipped_lines + 1).unwrap(),
                line: line.into(),
                file_offset: offset,
                line_offset,
                len,
            });
        }
        bail!("The offset exceeds the length of the file")
    }

    /// Get the content of the span as a `&str`.
    pub fn as_str(&self) -> &str {
        &self.line[self.line_offset..self.line_offset + self.len]
    }

    /// Obtain a string (with colors) of this span.
    pub fn to_string(&self, level: DiagnosticLevel) -> String {
        let mut result = String::new();

        result += &format!(
            "{}:{}:{}\n",
            self.file_name.display(),
            self.line_number,
            self.line_offset
        );

        let line_number = self.line_number.get().to_string();
        result += &format!("{} | {}\n", line_number, self.line);

        let pad = line_number.len() + 3 + self.line_offset;
        result += &" ".repeat(pad);

        let color = level.color();
        for _ in 0..(self.len.max(1)) {
            result += &format!("{}", "^".color(color).bold());
        }
        result += "\n";
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::CodeSpan;

    #[test]
    fn test_empty() {
        let span = CodeSpan::from_str("file.txt", "", 0, 0);
        assert!(span.is_err());
    }

    #[test]
    fn test_first_line_from_start() {
        let span = CodeSpan::from_str("file.txt", "content", 0, 2).unwrap();
        assert_eq!(
            span,
            CodeSpan {
                file_name: "file.txt".into(),
                line_number: 1.try_into().unwrap(),
                line: "content".to_string(),
                file_offset: 0,
                line_offset: 0,
                len: 2
            }
        );
        assert_eq!(span.as_str(), "co");
    }

    #[test]
    fn test_first_line_till_end() {
        let span = CodeSpan::from_str("file.txt", "content", 0, 7).unwrap();
        assert_eq!(
            span,
            CodeSpan {
                file_name: "file.txt".into(),
                line_number: 1.try_into().unwrap(),
                line: "content".to_string(),
                file_offset: 0,
                line_offset: 0,
                len: 7
            }
        );
        assert_eq!(span.as_str(), "content");
    }

    #[test]
    fn test_first_line_with_other_lines() {
        let span = CodeSpan::from_str("file.txt", "content\nnope", 0, 7).unwrap();
        assert_eq!(
            span,
            CodeSpan {
                file_name: "file.txt".into(),
                line_number: 1.try_into().unwrap(),
                line: "content".to_string(),
                file_offset: 0,
                line_offset: 0,
                len: 7
            }
        );
        assert_eq!(span.as_str(), "content");
    }

    #[test]
    fn test_second_line() {
        let span = CodeSpan::from_str("file.txt", "content\nnope", 9, 2).unwrap();
        assert_eq!(
            span,
            CodeSpan {
                file_name: "file.txt".into(),
                line_number: 2.try_into().unwrap(),
                line: "nope".to_string(),
                file_offset: 9,
                line_offset: 1,
                len: 2
            }
        );
        assert_eq!(span.as_str(), "op");
    }

    #[test]
    fn test_second_line_with_newline() {
        let span = CodeSpan::from_str("file.txt", "content\nnope\n", 9, 2).unwrap();
        assert_eq!(
            span,
            CodeSpan {
                file_name: "file.txt".into(),
                line_number: 2.try_into().unwrap(),
                line: "nope".to_string(),
                file_offset: 9,
                line_offset: 1,
                len: 2
            }
        );
        assert_eq!(span.as_str(), "op");
    }
}
