use crate::ir::*;
use crate::sem;

use super::DiagnosticContext;

#[derive(Clone, Default)]
pub struct Env {
    refs: Vec<Ir<Var>>,
    outer: Option<Box<Env>>,

    pub cfg: Ir<sem::Cfg>,
    pub cur_io: Option<Ir<IoKw>>,
    pub loc: Ir<Loc>,
}

impl Env {
    pub fn root(cfg: sem::Cfg) -> Self {
        Self {
            cfg: Ir::new(cfg),
            ..Default::default()
        }
    }

    pub fn declare(self: &mut Self, var: &Ir<Var>, dgns: &mut DiagnosticContext) {
        match self.maybe_resolve(&var.name) {
            None => {
                self.refs.push(var.clone());
            }
            Some(other_var) => dgns.error(
                &format!("variable `{}` already defined", var.name.ident.to_string()),
                vec![
                    dgns.error_ann(
                        "cannot re-define a variable in scope",
                        var.name.ident.span(),
                    ),
                    dgns.info_ann("was defined here", other_var.name.ident.span()),
                ],
                vec![],
            ),
        }
    }

    pub fn declare_expr(self: &mut Self, expr: &Ir<DataDefExpr>, dgns: &mut DiagnosticContext) {
        let var = &expr.root_var;
        self.declare(
            &Ir::new(Var {
                kind: VarKind::Data { def: var.clone() },
                ty: var.ty.clone(),
                name: var.name.clone(),
            }),
            dgns,
        )
    }

    pub fn resolve(self: &Self, name: &Ir<Name>, dgns: &mut DiagnosticContext) -> Ir<Var> {
        match self.maybe_resolve(name) {
            Some(var) => var,
            None => {
                dgns.error(
                    &format!(
                        "no variable named `{}` found in the current scope",
                        name.ident.to_string()
                    ),
                    vec![dgns.error_ann("not found in this scope", name.ident.span())],
                    vec![],
                );
                Ir::new(Var {
                    name: name.clone(),
                    ty: Ir::new(ExprTy::Err),
                    kind: VarKind::Err,
                })
            }
        }
    }

    fn maybe_resolve(self: &Self, name: &Ir<Name>) -> Option<Ir<Var>> {
        self.refs
            .iter()
            .find(|r| r.name.ident == name.ident)
            .map(|r| r.clone())
            .or(self.outer.as_ref().and_then(|s| s.maybe_resolve(name)))
    }

    pub fn for_body(self: &Self, range: Ir<Range>) -> Self {
        Self {
            refs: vec![Ir::new(Var {
                name: range.index.clone(),
                ty: range.bound.val.ty.clone(),
                kind: VarKind::Index {
                    range: range.clone(),
                },
            })],
            outer: Some(Box::new(self.clone())),
            loc: Ir::new(Loc::For {
                range: range.clone(),
                parent: self.loc.clone(),
            }),
            cur_io: self.cur_io.clone(),
            cfg: self.cfg.clone(),
        }
    }

    pub fn io(self: &Self, step: &Ir<IoKw>) -> Self {
        Self {
            outer: Some(Box::new(self.clone())),
            loc: self.loc.clone(),
            cur_io: Some(step.clone()),
            cfg: self.cfg.clone(),
            ..Default::default()
        }
    }

    pub fn data_env(self: &Self, ty: &Ir<AtomTy>) -> DataDefEnv {
        DataDefEnv {
            outer: Box::new(self.clone()),
            ty: Ir::new(ExprTy::Atom {
                atom_ty: ty.clone(),
            }),
            loc: self.loc.clone(),
        }
    }
}

#[derive(Clone)]
pub enum Loc {
    Main,
    For { range: Ir<Range>, parent: Ir<Loc> },
}

impl Default for Loc {
    fn default() -> Self {
        Self::Main
    }
}

/// Special environment used when defining new variables.
///
/// E.g., the environment in which `A[i]` is compiled,
/// in the statement `item A[i][j]: i32;`.
#[derive(Clone)]
pub struct DataDefEnv {
    pub outer: Box<Env>,
    pub ty: Ir<ExprTy>,
    pub loc: Ir<Loc>,
}
