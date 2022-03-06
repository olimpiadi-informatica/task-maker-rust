mod span;

use std::fmt::{Display, Formatter};

use colored::{Color, Colorize};
use serde::{Deserialize, Serialize};

pub use span::CodeSpan;

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticLevel::Error => "Error",
            DiagnosticLevel::Warning => "Warning",
        }
    }

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

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Diagnostic {
    level: DiagnosticLevel,
    message: String,
    note: Option<String>,
    help: Option<String>,
    help_attachment: Option<Vec<u8>>,
    code_span: Option<CodeSpan>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
            note: None,
            help: None,
            help_attachment: None,
            code_span: None,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            note: None,
            help: None,
            help_attachment: None,
            code_span: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_help_attachment(mut self, attachment: Vec<u8>) -> Self {
        self.help_attachment = Some(attachment);
        self
    }

    pub fn with_code_span(mut self, code_span: CodeSpan) -> Self {
        self.code_span = Some(code_span);
        self
    }

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
            if lines.len() > 4 {
                for index in [0, 1] {
                    writeln!(f, "{:>pad$} | {}", index + 1, lines[index], pad = pad)?;
                }
                writeln!(f, "{:>pad$} |", "...", pad = pad)?;
                for index in [lines.len() - 2, lines.len() - 1] {
                    writeln!(f, "{:>pad$} | {}", index + 1, lines[index], pad = pad)?;
                }
            } else {
                for (index, line) in lines.iter().enumerate() {
                    writeln!(f, "{:>pad$} | {}", index + 1, line, pad = pad)?;
                }
            }
        }
        if let Some(code_span) = &self.code_span {
            for line in code_span.to_string(self.level).lines() {
                writeln!(f, "{:>pad$} {}", "", line, pad = pad + 1)?;
            }
        }
        Ok(())
    }

    pub fn level(&self) -> DiagnosticLevel {
        self.level
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.print(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticContext {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
