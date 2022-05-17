use anyhow::Context;
use anyhow::Error;
use clap::ArgEnum;
use clap::Parser;
use codemap::CodeMap;
use std::fs::read_to_string;
use std::io;
use std::path::PathBuf;

use crate::ast;
use crate::compile;
use crate::dgns;
use crate::sem;
use crate::spec::ir::Spec;

#[derive(Parser, Debug, Clone)]
pub struct SpecOpt {
    #[clap(long, default_value = "gen/IOSPEC")]
    pub spec: PathBuf,
    #[clap(long)]
    pub cfg: Vec<String>,
    #[clap(long, arg_enum, default_value = "always")]
    pub color: ColorOpt,
}

#[derive(ArgEnum, Debug, Clone, Copy)]
pub enum ColorOpt {
    Always,
    Never,
}

impl SpecOpt {
    pub fn load(
        self,
        stderr: &mut dyn io::Write,
        base_cfg: Vec<String>,
    ) -> Result<(Spec, dgns::DiagnosticContext), Error> {
        let source = read_to_string(&self.spec).context("cannot read file")?;
        let mut code_map = CodeMap::new();
        let file = code_map.add_file(self.spec.to_string_lossy().into(), source.clone());
        let mut dgns = dgns::DiagnosticContext {
            spec_file: file,
            stderr,
            color: matches!(self.color, ColorOpt::Always),
        };

        let ast: ast::Spec = syn::parse_str(&source).map_err(|e| {
            dgns.error(
                &e.to_string(),
                vec![dgns.error_ann("here", e.span())],
                vec![],
            );
            e
        })?;

        let ir = compile::compile(
            &ast,
            &mut dgns,
            sem::Cfg(base_cfg.into_iter().chain(self.cfg).collect()),
        )
        .map_err(|_| anyhow::anyhow!("compilation stopped due to previous errors"))?;

        Ok((ir, dgns))
    }
}
