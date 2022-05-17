use crate::ast;
use crate::ir::*;

#[derive(Debug)]
pub struct CallMetaStmt {
    pub kw: ast::kw::call,
    pub name: Name,
    pub paren: syn::token::Paren,
    pub arg_commas: Vec<syn::Token![,]>,
    pub args: Vec<Ir<CallArg>>,
    pub ret: CallRet,
    pub semi: syn::Token![;],
}

#[derive(Debug)]
pub struct CallRet(pub Option<CallRetExpr>);

#[derive(Debug)]
pub struct CallArg {
    pub name: Ir<Name>,
    pub eq: syn::Token![=],
    pub kind: CallArgKind,
}

#[derive(Debug)]
pub enum CallArgKind {
    Value(CallByValueArg),
    Reference(CallByReferenceArg),
}

#[derive(Debug)]
pub struct CallByValueArg {
    pub expr: Expr,
}

#[derive(Debug)]
pub struct CallByReferenceArg {
    pub amp: syn::Token![&],
    pub expr: Expr,
}

#[derive(Debug)]
pub struct CallRetExpr {
    pub arrow: syn::Token![->],
    pub kind: CallRetKind,
}

#[derive(Debug)]
pub enum CallRetKind {
    Single(SingleCallRet),
    Tuple(TupleCallRet),
}

#[derive(Debug)]
pub struct SingleCallRet {
    pub expr: Expr,
}

#[derive(Debug)]
pub struct TupleCallRet {
    pub paren: syn::token::Paren,
    pub items: Vec<Expr>,
    pub item_commas: Vec<syn::Token![,]>,
}
