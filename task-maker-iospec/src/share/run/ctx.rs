use crate::compile::DiagnosticContext;

use super::io::*;

pub struct Context<'a> {
    pub input_source: IoSource,
    pub output_source: Option<IoSource>,
    pub dgns: DiagnosticContext<'a>,
}
