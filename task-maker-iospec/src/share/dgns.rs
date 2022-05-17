//! Utilities to generate diagnostic messages.

use std::io::Write;
use std::sync::Arc;

use annotate_snippets::display_list::DisplayList;
use annotate_snippets::display_list::FormatOptions;
use annotate_snippets::snippet::*;
use anyhow::Context;
use codemap::File;
use proc_macro2::LineColumn;

pub use proc_macro2::Span;

pub trait HasSpan {
    fn span(self: &Self) -> Span;
}
pub trait TryHasSpan {
    fn try_span(self: &Self) -> Option<Span>;
}

pub struct DiagnosticContext<'a> {
    pub spec_file: Arc<File>,
    pub stderr: &'a mut dyn Write,
    pub color: bool,
}

impl DiagnosticContext<'_> {
    pub fn error(
        self: &mut Self,
        message: &str,
        annotations: Vec<SourceAnnotation>,
        footer: Vec<Annotation>,
    ) {
        self.stderr
            .write_fmt(format_args!(
                "{}\n",
                DisplayList::from(Snippet {
                    title: Some(Annotation {
                        id: None,
                        label: Some(message),
                        annotation_type: AnnotationType::Error,
                    }),
                    footer,
                    slices: vec![Slice {
                        source: self.spec_file.source(),
                        line_start: 1,
                        origin: Some(self.spec_file.name()),
                        fold: true,
                        annotations,
                    }],
                    opt: FormatOptions {
                        color: self.color,
                        ..Default::default()
                    },
                }),
            ))
            .context("while writing a diagnostic message")
            .unwrap();
    }

    pub fn footer<'a>(
        self: &Self,
        annotation_type: AnnotationType,
        message: &'a str,
    ) -> Annotation<'a> {
        Annotation {
            annotation_type,
            label: Some(message),
            id: None,
        }
    }

    pub fn note_footer<'a>(self: &Self, message: &'a str) -> Annotation<'a> {
        self.footer(AnnotationType::Note, message)
    }

    pub fn help_footer<'a>(self: &Self, message: &'a str) -> Annotation<'a> {
        self.footer(AnnotationType::Help, message)
    }

    pub fn error_ann<'a>(self: &Self, label: &'a str, span: Span) -> SourceAnnotation<'a> {
        SourceAnnotation {
            annotation_type: AnnotationType::Error,
            label,
            range: (self.pos(span.start()), self.pos(span.end())),
        }
    }

    pub fn info_ann<'a>(self: &Self, label: &'a str, span: Span) -> SourceAnnotation<'a> {
        SourceAnnotation {
            annotation_type: AnnotationType::Info,
            label,
            range: (self.pos(span.start()), self.pos(span.end())),
        }
    }

    fn pos(self: &Self, lc: LineColumn) -> usize {
        let line_start = self.spec_file.line_span(lc.line - 1).low() - self.spec_file.span.low();
        line_start as usize + lc.column
    }
}
