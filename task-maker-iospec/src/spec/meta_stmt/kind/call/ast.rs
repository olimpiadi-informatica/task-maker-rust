use syn::Token;

use crate::ast::*;

#[derive(Debug, Clone)]
pub struct CallMetaStmt {
    pub kw: kw::call,
    pub name: Name,
    pub paren: syn::token::Paren,
    pub args: syn::punctuated::Punctuated<CallArg, Token![,]>,
    pub ret: Option<CallRet>,
    pub semi: syn::Token![;],
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Name,
    pub eq: syn::Token![=],
    pub kind: CallArgKind,
}

#[derive(Debug, Clone)]
pub enum CallArgKind {
    Value(CallByValueArg),
    Reference(CallByReferenceArg),
}

#[derive(Debug, Clone)]
pub struct CallByValueArg {
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct CallByReferenceArg {
    pub amp: Token![&],
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct CallRet {
    pub arrow: syn::Token![->],
    pub kind: CallRetKind,
}

#[derive(Debug, Clone)]
pub enum CallRetKind {
    Single(SingleCallRet),
    Tuple(TupleCallRet),
}

#[derive(Debug, Clone)]
pub struct SingleCallRet {
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct TupleCallRet {
    pub paren: syn::token::Paren,
    pub items: syn::punctuated::Punctuated<Expr, Token![,]>,
}
