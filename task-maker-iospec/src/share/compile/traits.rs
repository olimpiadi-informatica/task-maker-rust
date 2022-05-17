use crate::dgns::DiagnosticContext;
use crate::ir::*;

use super::*;

#[derive(Clone, Copy)]
pub struct CompileStop;

pub type Result<T> = std::result::Result<T, CompileStop>;

pub trait CompileFrom<T, E = Env>
where
    Self: Sized,
{
    fn compile(ast: &T, env: &E, dgns: &mut DiagnosticContext) -> Result<Self>;
}

impl<T, E, U> CompileFrom<T, E> for Ir<U>
where
    T: CompileInto<U, E>,
{
    fn compile(ast: &T, env: &E, dgns: &mut DiagnosticContext) -> Result<Self> {
        Ok(Ir::new(ast.compile(env, dgns)?))
    }
}

pub trait CompileInto<T, E = Env> {
    fn compile(&self, env: &E, dgns: &mut DiagnosticContext) -> Result<T>;
}

impl<U, T, E> CompileInto<U, E> for T
where
    U: CompileFrom<T, E>,
{
    fn compile(&self, env: &E, dgns: &mut DiagnosticContext) -> Result<U> {
        U::compile(self, env, dgns)
    }
}

pub trait AnalyzeFrom<T> {
    fn analyze(ir: &T, dgns: &mut DiagnosticContext) -> Self;
}

impl<T, U> AnalyzeFrom<Ir<T>> for U
where
    T: AnalyzeInto<U>,
{
    fn analyze(ir: &Ir<T>, dgns: &mut DiagnosticContext) -> Self {
        ir.as_ref().analyze(dgns)
    }
}

pub trait AnalyzeInto<T> {
    fn analyze(&self, dgns: &mut DiagnosticContext) -> T;
}

impl<U, T> AnalyzeInto<U> for T
where
    U: AnalyzeFrom<T>,
{
    fn analyze(&self, dgns: &mut DiagnosticContext) -> U {
        U::analyze(self, dgns)
    }
}
