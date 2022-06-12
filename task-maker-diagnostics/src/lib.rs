//! This crate contains the code for producing human-friendly and machine-friendly diagnostic
//! messages.

#![deny(missing_docs)]

mod span;

use std::fmt::{Display, Formatter};

use colored::{Color, Colorize};
use serde::{Deserialize, Serialize};

pub use span::CodeSpan;

/// The level of the message.
///
/// This influences the color of the output, and the order in which the diagnostics are shown.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    /// The message is just a warning.
    Warning,
    /// The message is an error.
    Error,
}

impl DiagnosticLevel {
    /// Return a human-friendly version of this level.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticLevel::Error => "Error",
            DiagnosticLevel::Warning => "Warning",
        }
    }

    /// The color in which this message should be printed.
    pub fn color(&self) -> Color {
        match self {
            DiagnosticLevel::Warning => Color::BrightYellow,
            DiagnosticLevel::Error => Color::BrightRed,
        }
    }
}

impl Display for DiagnosticLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A [`Diagnostic`] is a message, with some extra information attached, such as the message level,
/// additional information about what happened or some help on how to fix the issue.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The level of this message.
    level: DiagnosticLevel,
    /// The main message to report.
    message: String,
    /// Additional notes to show next to the main message.
    note: Option<String>,
    /// Some help for diagnosing the problem.
    help: Option<String>,
    /// The content of a file that can help fixing the issue. This can be multiline, but only the
    /// initial and final part of this will be shown.
    help_attachment: Option<Vec<u8>>,
    /// Spans to the relevant parts of the code of where the error is generated.
    code_spans: Vec<CodeSpan>,
}

impl Diagnostic {
    /// Create a new [`Diagnostic`] with [`DiagnosticLevel::Error`].
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
            note: None,
            help: None,
            help_attachment: None,
            code_spans: Default::default(),
        }
    }

    /// Create a new [`Diagnostic`] with [`DiagnosticLevel::Warning`].
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            note: None,
            help: None,
            help_attachment: None,
            code_spans: Default::default(),
        }
    }

    /// Attach a note to the diagnostic.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Attach a help message to the diagnostic.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Attach a file to the diagnostic.
    pub fn with_help_attachment(mut self, attachment: Vec<u8>) -> Self {
        self.help_attachment = Some(attachment);
        self
    }

    /// Attach a [`CodeSpan`] to the diagnostic.
    pub fn with_code_span(mut self, code_span: CodeSpan) -> Self {
        self.code_spans.push(code_span);
        self
    }

    /// Print this diagnostic to the formatter. This is used by the [`std::fmt::Display`] trait.
    pub fn print(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // TODO: additional printing options (e.g. no colors, compact, ...)
        let level = self.level.as_str();
        let pad = level.len();
        writeln!(
            f,
            "{}: {}",
            level.color(self.level.color()).bold(),
            self.message
        )?;
        if let Some(note) = &self.note {
            write!(f, "{:>pad$}: ", "Note".bold(), pad = pad)?;
            let mut lines = note.lines();
            if let Some(line) = lines.next() {
                writeln!(f, "{}", line)?;
            }
            for line in lines {
                writeln!(f, "{:>pad$}  {}", "", line, pad = pad)?;
            }
        }
        if let Some(help) = &self.help {
            writeln!(f, "{:>pad$}: {}", "Help".bold(), help, pad = pad)?;
        }
        if let Some(attachment) = &self.help_attachment {
            let attachment = String::from_utf8_lossy(attachment);
            let lines: Vec<_> = attachment.lines().collect();
            let context_lines = 5;
            if lines.len() > context_lines + 1 + context_lines {
                for (index, line) in lines.iter().enumerate().take(context_lines) {
                    writeln!(f, "{:>pad$} | {}", index + 1, line, pad = pad)?;
                }
                writeln!(f, "{:>pad$} |", "...", pad = pad)?;
                for (index, line) in lines.iter().enumerate().skip(lines.len() - context_lines) {
                    writeln!(f, "{:>pad$} | {}", index + 1, line, pad = pad)?;
                }
            } else {
                for (index, line) in lines.iter().enumerate() {
                    writeln!(f, "{:>pad$} | {}", index + 1, line, pad = pad)?;
                }
            }
        }
        for code_span in &self.code_spans {
            for line in code_span.to_string(self.level).lines() {
                writeln!(f, "{:>pad$} {}", "", line, pad = pad + 1)?;
            }
        }
        Ok(())
    }

    /// Get the level of the diagnostic.
    pub fn level(&self) -> DiagnosticLevel {
        self.level
    }

    /// Get the message of this diagnostic.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.print(f)
    }
}

/// The context that contains all the emitted diagnostic messages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticContext {
    /// The list of emitted diagnostics.
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticContext {
    /// Build a new, empty, [`DiagnosticContext`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new [`Diagnostic`] to this context.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Return the list of diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
