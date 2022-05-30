pub mod ast {
    use crate::ast::*;

    #[derive(Copy, Clone, Debug)]
    pub enum RelOp {
        Eq(syn::Token![==]),
        Ne(syn::Token![!=]),
        Lt(syn::Token![<]),
        Le(syn::Token![<=]),
        Gt(syn::Token![>]),
        Ge(syn::Token![>=]),
    }

    #[derive(Debug, Clone)]
    pub struct RelChainExpr {
        pub chain: syn::punctuated::Punctuated<Expr, RelOp>,
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for RelOp {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(syn::Token![==]) {
                Self::Eq(input.parse()?)
            } else if la.peek(syn::Token![!=]) {
                Self::Ne(input.parse()?)
            } else if la.peek(syn::Token![<=]) {
                Self::Le(input.parse()?)
            } else if la.peek(syn::Token![>=]) {
                Self::Ge(input.parse()?)
            } else if la.peek(syn::Token![<]) {
                Self::Lt(input.parse()?)
            } else if la.peek(syn::Token![>]) {
                Self::Gt(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    pub type RelOp = super::ast::RelOp;

    #[derive(Debug, Clone)]
    pub struct RelExpr(pub Ir<Expr>, pub RelOp, pub Ir<Expr>);

    #[derive(Debug)]
    pub struct RelChainExpr {
        pub first: Ir<Expr>,
        pub rest_chain: Vec<(RelOp, Ir<Expr>)>,
        pub rels: Vec<RelExpr>,
    }
}
mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::RelChainExpr> for ExprKind {
        fn compile(
            ast: &ast::RelChainExpr,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::RelChainExpr { chain } = ast;

            let (values, ops) = unzip_punctuated(chain.clone());
            let values: Vec<Ir<Expr>> = values
                .iter()
                .map(|v| v.compile(env, dgns))
                .collect::<Result<_>>()?;

            let first = values.first().unwrap().clone();
            let rest_chain = ops
                .iter()
                .cloned()
                .zip(values.iter().skip(1).cloned())
                .collect();

            let rels: Vec<_> = values
                .iter()
                .skip(1)
                .zip(values.iter())
                .zip(ops.into_iter())
                .map(|((right, left), op)| RelExpr(left.clone(), op, right.clone()))
                .collect();

            // TODO: type

            Ok(ExprKind::RelChain(RelChainExpr {
                first,
                rest_chain,
                rels,
            }))
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;
    use crate::sem;

    impl RelOp {
        pub fn apply(self: &Self, left: i64, right: i64) -> bool {
            match self {
                RelOp::Eq(_) => left == right,
                RelOp::Ne(_) => left != right,
                RelOp::Lt(_) => left < right,
                RelOp::Le(_) => left <= right,
                RelOp::Gt(_) => left > right,
                RelOp::Ge(_) => left >= right,
            }
        }
    }

    impl Eval for RelChainExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            Ok(ExprVal::Atom(sem::AtomVal::new(
                sem::AtomTy::Bool,
                match self
                    .rels
                    .iter()
                    .map::<Result<_, Stop>, _>(|RelExpr(left, op, right)| {
                        let left = left.eval(state, ctx)?.unwrap_value_i64();
                        let right = right.eval(state, ctx)?.unwrap_value_i64();

                        Ok(op.apply(left, right))
                    })
                    .find(|r| if let Ok(false) = r { true } else { false })
                {
                    Some(res) => {
                        res?;
                        0
                    }
                    None => 1,
                },
            )))
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for RelOp {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                Self::Eq(_) => gen!(ctx, "=="),
                Self::Ne(_) => gen!(ctx, "!="),
                Self::Lt(_) => gen!(ctx, "<"),
                Self::Le(_) => gen!(ctx, "<="),
                Self::Gt(_) => gen!(ctx, ">"),
                Self::Ge(_) => gen!(ctx, ">="),
            }
        }
    }

    lang_mixin!(Inspect, RelOp, CommonMixin);

    impl<L> Gen<CommonMixin<'_, L>> for RelChainExpr
    where
        RelExpr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { rels, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            gen!(
                ctx,
                "{}" % (&Punctuated(rels.iter().cloned().collect(), " && "))
            )
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for RelExpr
    where
        Expr: Gen<L>,
        RelOp: Gen<L>,
        ExprKind: Gen<L>, // FIXME: should not be needed
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self(lexpr, op, rexpr) = self;
            gen!(ctx, "{} {} {}" % (lexpr, op, rexpr))
        }
    }

    lang_mixin!(Inspect, RelChainExpr, CommonMixin);
    lang_mixin!(Inspect, RelExpr, CommonMixin);
}
