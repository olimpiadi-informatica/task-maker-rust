//! Implements compilation from AST to IR.

mod env;
mod traits;
mod util;

pub use crate::dgns::*;
use crate::sem;

pub use env::*;
pub use traits::*;
pub use util::*;

pub fn compile(
    ast: &crate::ast::Spec,
    dgns: &mut DiagnosticContext,
    cfg: sem::Cfg,
) -> Result<crate::ir::Spec> {
    ast.compile(&env::Env::root(cfg), dgns)
}
