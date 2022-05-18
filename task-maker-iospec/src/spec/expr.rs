pub mod ast {
    use crate::ast::*;

    /// AST of an expression.

    #[derive(Debug, Clone)]
    pub enum Expr {
        IntLit(IntLitExpr),
        Var(VarExpr),
        Subscript(SubscriptExpr),
        Paren(ParenExpr),
        Mul(MulExpr),
        Sum(SumExpr),
        RelChain(RelChainExpr),
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for Expr {
        fn parse(input: ParseStream) -> Result<Self> {
            Self::parse_rel(input)
        }
    }

    impl Expr {
        fn parse_atomic(input: ParseStream) -> Result<Self> {
            let mut current = if input.peek(syn::token::Paren) {
                let inner_input;

                Self::Paren(ParenExpr {
                    paren: syn::parenthesized!(inner_input in input),
                    inner: Box::new(inner_input.parse()?),
                })
            } else if input.peek(syn::Lit) {
                let token: syn::LitInt = input.parse()?;
                let value_i64 = token.base10_parse()?;
                Self::IntLit(IntLitExpr { token, value_i64 })
            } else {
                Self::Var(VarExpr {
                    name: input.parse()?,
                })
            };

            while input.peek(syn::token::Bracket) {
                let index_input;
                current = Self::Subscript(SubscriptExpr {
                    array: Box::new(current),
                    bracket: syn::bracketed!(index_input in input),
                    index: index_input.parse()?,
                });
            }
            Ok(current)
        }

        fn parse_mul(input: ParseStream) -> Result<Self> {
            let first: Self = Self::parse_atomic(input)?;
            Ok(if input.peek(syn::Token![*]) {
                let mut chain = syn::punctuated::Punctuated::<Self, syn::Token![*]>::new();
                chain.push_value(first);
                while input.peek(syn::Token![*]) {
                    chain.push_punct(input.parse()?);
                    chain.push_value(Self::parse_atomic(input)?);
                }
                Self::Mul(MulExpr { factors: chain })
            } else {
                first
            })
        }

        fn parse_sum(input: ParseStream) -> Result<Self> {
            let first_sign: Option<Sign> = if Self::peek_sign(input) {
                Some(input.parse()?)
            } else {
                None
            };

            let first: Self = Self::parse_mul(input)?;

            Ok(if first_sign.is_some() || Self::peek_sign(input) {
                let mut chain = syn::punctuated::Punctuated::<Self, Sign>::new();
                chain.push_value(first);
                while Self::peek_sign(input) {
                    chain.push_punct(input.parse()?);
                    chain.push_value(Self::parse_mul(input)?);
                }
                Self::Sum(SumExpr {
                    first_sign,
                    terms: chain,
                })
            } else {
                first
            })
        }

        fn parse_rel(input: ParseStream) -> Result<Self> {
            let first: Self = Self::parse_sum(input)?;

            Ok(if Self::peek_rel_op(input) {
                let mut chain = syn::punctuated::Punctuated::<Self, RelOp>::new();
                chain.push_value(first);
                while Self::peek_rel_op(input) {
                    chain.push_punct(input.parse()?);
                    chain.push_value(Self::parse_sum(input)?);
                }
                Self::RelChain(RelChainExpr { chain })
            } else {
                first
            })
        }

        fn peek_sign(input: ParseStream) -> bool {
            input.peek(syn::Token![+]) || input.peek(syn::Token![-])
        }

        fn peek_rel_op(input: ParseStream) -> bool {
            [
                input.peek(syn::Token![==]),
                input.peek(syn::Token![!=]),
                input.peek(syn::Token![<=]),
                input.peek(syn::Token![>=]),
                input.peek(syn::Token![<]),
                input.peek(syn::Token![>]),
            ]
            .iter()
            .any(|b| *b)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rel_chain() {
            let _: Expr = syn::parse_str("a == 1").unwrap();
            let _: Expr = syn::parse_str("1 <= 2 <= 3").unwrap();
        }
    }
}

pub mod ir {
    use crate::ir::*;

    /// IR of a value (rvalue, atomic or aggregate) defined by an expression.
    /// E.g., `A[B[i]]` in `... for i upto A[B[j]][k] { ... } ...`.
    #[derive(Debug)]
    pub struct Expr {
        pub kind: ExprKind,
        pub ty: Ir<ExprTy>,
    }

    #[derive(Default, Debug)]
    pub enum ExprKind {
        Lit(LitExpr),
        Var(VarExpr),
        Subscript(SubscriptExpr),
        Paren(ParenExpr),
        Mul(MulExpr),
        Sum(SumExpr),
        RelChain(RelChainExpr),
        #[default]
        Err,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;
    use crate::sem;

    impl CompileFrom<ast::Expr> for ExprKind {
        fn compile(ast: &ast::Expr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            Ok(match ast {
                ast::Expr::IntLit(expr) => expr.compile(env, dgns)?,
                ast::Expr::Var(expr) => expr.compile(env, dgns)?,
                ast::Expr::Subscript(expr) => expr.compile(env, dgns)?,
                ast::Expr::Paren(expr) => expr.compile(env, dgns)?,
                ast::Expr::Mul(expr) => expr.compile(env, dgns)?,
                ast::Expr::Sum(expr) => expr.compile(env, dgns)?,
                ast::Expr::RelChain(expr) => expr.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::Expr> for Expr {
        fn compile(ast: &ast::Expr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let kind: ExprKind = ast.compile(env, dgns)?;

            Ok(Expr {
                ty: match &kind {
                    ExprKind::Var(VarExpr { var, .. }) => var.ty.clone(),
                    ExprKind::Subscript(SubscriptExpr { array, index, .. }) => {
                        match array.ty.as_ref() {
                            ExprTy::Array { item, range } => {
                                match index.ty.as_ref() {
                                    ExprTy::Atom { atom_ty }
                                        if atom_ty.sem == range.bound.ty.sem =>
                                    {
                                        ()
                                    }
                                    _ => dgns.error(
                                        &format!(
                                            "expected index of type `{}`, got `{}`",
                                            quote_hir(range.bound.ty.as_ref()),
                                            quote_hir(index.ty.as_ref()),
                                        ),
                                        vec![
                                            dgns.error_ann(
                                                &format!(
                                                    "invalid index type `{}`",
                                                    quote_hir(index.ty.as_ref())
                                                ),
                                                index.span(),
                                            ),
                                            dgns.info_ann("array range", range.span()),
                                            dgns.info_ann("expected type", range.bound.ty.span()),
                                        ]
                                        .into_iter()
                                        .chain(match index.ty.as_ref() {
                                            ExprTy::Atom { atom_ty } => {
                                                Some(dgns.info_ann("got this type", atom_ty.span()))
                                            }
                                            _ => None,
                                        })
                                        .collect(),
                                        vec![],
                                    ),
                                }
                                item.clone()
                            }
                            ExprTy::Err => Default::default(),
                            _ => {
                                dgns.error(
                                    &format!(
                                        "cannot index into a value of non-array type `{}`",
                                        quote_hir(array.ty.as_ref()),
                                    ),
                                    vec![dgns.error_ann("must be an array", array.span())],
                                    vec![],
                                );

                                Default::default()
                            }
                        }
                    }
                    ExprKind::Lit(LitExpr { ty, .. })
                    | ExprKind::Mul(MulExpr { ty, .. })
                    | ExprKind::Sum(SumExpr { ty, .. }) => Ir::new(ExprTy::Atom {
                        atom_ty: ty.clone(),
                    }),
                    ExprKind::RelChain(RelChainExpr { rels, .. }) => Ir::new(ExprTy::Atom {
                        atom_ty: Ir::new(AtomTy {
                            sem: Some(sem::AtomTy::Bool),
                            kind: AtomTyKind::Rel {
                                rels: (*rels).clone(),
                            },
                        }),
                    }),
                    ExprKind::Paren(ParenExpr { inner, .. }) => inner.ty.clone(),
                    ExprKind::Err => Default::default(),
                },
                kind,
            })
        }
    }
}

mod dgns {
    use syn::spanned::Spanned;

    use crate::ast;
    use crate::dgns::*;
    use crate::ir::*;

    impl HasSpan for Expr {
        fn span(self: &Self) -> Span {
            self.kind.span()
        }
    }

    impl HasSpan for ExprKind {
        fn span(self: &Self) -> Span {
            match self {
                Self::Var(VarExpr { name, .. }) => name.ident.span(),
                Self::Subscript(SubscriptExpr { array, bracket, .. }) => {
                    array.span().join(bracket.span).unwrap()
                }
                Self::Lit(LitExpr { token, .. }) => token.span(),
                Self::Paren(ParenExpr { paren, .. }) => paren.span,
                Self::Mul(MulExpr { factors, .. }) => factors
                    .first()
                    .unwrap()
                    .span()
                    .join(factors.last().unwrap().span())
                    .unwrap(),
                Self::Err => panic!(),
                Self::Sum(SumExpr { terms, .. }) => {
                    let extrema: Vec<_> = [terms.first(), terms.last()]
                        .iter()
                        .map(|t| t.unwrap())
                        .map(|(sign, term)| {
                            let sign_span = match sign {
                                Sign::Plus(Some(op)) => Some(op.span()),
                                Sign::Minus(op) => Some(op.span()),
                                Sign::Plus(None) => None,
                            };
                            if let Some(span) = sign_span {
                                term.span().join(span).unwrap()
                            } else {
                                term.span()
                            }
                        })
                        .collect();
                    extrema[0].join(extrema[1]).unwrap()
                }
                Self::RelChain(RelChainExpr { rels, .. }) => rels
                    .first()
                    .unwrap()
                    .0
                    .span()
                    .join(rels.last().unwrap().2.span())
                    .unwrap(),
            }
        }
    }

    impl HasSpan for ast::Expr {
        fn span(self: &Self) -> Span {
            match self {
                Self::Var(ast::VarExpr { name, .. }) => name.ident.span(),
                Self::Subscript(ast::SubscriptExpr { array, bracket, .. }) => {
                    array.span().join(bracket.span).unwrap()
                }
                Self::IntLit(ast::IntLitExpr { token, .. }) => token.span(),
                Self::Paren(ast::ParenExpr { paren, .. }) => paren.span,
                Self::Mul(ast::MulExpr { factors, .. }) => factors
                    .first()
                    .unwrap()
                    .span()
                    .join(factors.last().unwrap().span())
                    .unwrap(),
                Self::Sum(ast::SumExpr {
                    first_sign, terms, ..
                }) => first_sign
                    .as_ref()
                    .map(|s| s.span())
                    .unwrap_or(terms.first().unwrap().span())
                    .span()
                    .join(terms.last().unwrap().span())
                    .unwrap(),
                Self::RelChain(ast::RelChainExpr { chain }) => chain
                    .first()
                    .unwrap()
                    .span()
                    .join(chain.last().unwrap().span())
                    .unwrap(),
            }
        }
    }
}

pub mod mem {
    use crate::mem::*;

    #[derive(Debug)]
    pub enum ExprVal<'a> {
        Atom(AtomVal),
        Array(&'a ArrayVal),
    }

    #[derive(Debug)]
    pub enum ArrayVal {
        AtomArray(Box<dyn AtomArray>),
        AggrArray(Vec<ArrayVal>),
        Empty,
    }

    impl<'a> ExprVal<'a> {
        pub fn unwrap_value_i64(&self) -> i64 {
            match self {
                ExprVal::Atom(atom) => atom.value_i64(),
                _ => unreachable!(),
            }
        }
    }

    #[derive(Debug)]
    pub enum ExprValMut<'a> {
        ConstAtom(AtomVal),
        Atom(&'a mut dyn AtomCell),
        Aggr(&'a mut ArrayVal),
    }
}

pub mod sem {
    use crate::sem::*;

    #[derive(Debug, Clone, Copy)]
    pub struct AtomVal {
        ty: crate::sem::AtomTy,
        value: i64,
    }

    impl AtomVal {
        pub fn new(ty: AtomTy, value: i64) -> AtomVal {
            Self::try_new(ty, value).unwrap()
        }

        pub fn try_new(ty: AtomTy, value: i64) -> Result<AtomVal, AtomTypeError> {
            let (min, max) = ty.value_range();

            if min <= value && value <= max {
                Ok(AtomVal { ty, value })
            } else {
                Err(AtomTypeError { ty, actual: value })
            }
        }

        pub fn value_i64(self: &Self) -> i64 {
            self.value
        }

        pub fn ty(self: &Self) -> AtomTy {
            self.ty
        }
    }

    #[derive(Debug)]
    pub struct AtomTypeError {
        pub ty: AtomTy,
        pub actual: i64,
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Eval for Expr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            match &self.kind {
                ExprKind::Lit(expr) => expr.eval(state, ctx),
                ExprKind::Var(expr) => expr.eval(state, ctx),
                ExprKind::Subscript(expr) => expr.eval(state, ctx),
                ExprKind::Paren(expr) => expr.eval(state, ctx),
                ExprKind::Mul(expr) => expr.eval(state, ctx),
                ExprKind::Sum(expr) => expr.eval(state, ctx),
                ExprKind::RelChain(expr) => expr.eval(state, ctx),
                ExprKind::Err => todo!(),
            }
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for Expr
    where
        ExprKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            self.kind.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    lang_mixin!(Inspect, Expr, CommonMixin);

    impl<L> Gen<CommonMixin<'_, L>> for ExprKind
    where
        VarExpr: Gen<L>,
        SubscriptExpr: Gen<L>,
        LitExpr: Gen<L>,
        ParenExpr: Gen<L>,
        MulExpr: Gen<L>,
        SumExpr: Gen<L>,
        RelChainExpr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                Self::Var(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Subscript(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Lit(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Paren(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Mul(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Sum(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::RelChain(expr) => expr.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Err => gen!(ctx, "<<compile-error>>"),
            }
        }
    }

    lang_mixin!(Inspect, ExprKind, CommonMixin);
}
