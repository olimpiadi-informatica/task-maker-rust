pub mod ast {
    use crate::ast::*;

    /// Plus or minus
    #[derive(Debug, Clone)]
    pub enum Sign {
        Plus(Option<syn::Token![+]>),
        Minus(syn::Token![-]),
    }

    #[derive(Debug, Clone)]
    pub struct SumExpr {
        pub first_sign: Option<Sign>,
        pub terms: syn::punctuated::Punctuated<Expr, Sign>,
    }
}

mod parse {
    use super::ast;
    use syn::parse::*;

    impl Parse for ast::Sign {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(syn::Token![+]) {
                Self::Plus(input.parse()?)
            } else if la.peek(syn::Token![-]) {
                Self::Minus(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    pub type Sign = super::ast::Sign;
    pub type TermExpr = (Sign, Ir<Expr>);

    #[derive(Debug)]
    pub struct SumExpr {
        pub terms: Vec<TermExpr>,
        pub ty: Ir<AtomTy>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::SumExpr> for ExprKind {
        fn compile(ast: &ast::SumExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::SumExpr { first_sign, terms } = ast;

            let (terms, ops) = unzip_punctuated(terms.clone());

            let terms = terms
                .iter()
                .map(|f| f.compile(env, dgns))
                .collect::<Result<Vec<_>>>()?;

            let terms: Vec<TermExpr> =
                std::iter::once(first_sign.as_ref().cloned().unwrap_or(Sign::Plus(None)))
                    .chain(ops.into_iter())
                    .zip(terms.into_iter())
                    .collect();

            let ty = terms
                .first()
                .as_ref()
                .unwrap()
                .1
                .ty
                .clone()
                .to_atom_ty()
                .unwrap(); // FIXME: type diagnostics

            Ok(ExprKind::Sum(SumExpr { terms, ty }))
        }
    }
}

mod dgns {
    use syn::spanned::Spanned;

    use super::ast::*;
    use crate::dgns::*;

    impl HasSpan for Sign {
        fn span(self: &Self) -> Span {
            match self {
                Sign::Plus(token) => token.span(),
                Sign::Minus(token) => token.span(),
            }
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;
    use crate::sem;

    impl Eval for SumExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            let ty = self.ty.sem.unwrap();
            let mut cur = sem::AtomVal::new(ty, 0);

            for (sign, term) in &self.terms {
                let term = term.eval(state, ctx)?.unwrap_value_i64();
                let term = match sign {
                    Sign::Plus(_) => term,
                    Sign::Minus(_) => term.checked_neg().ok_or_else(|| {
                        anyhow::anyhow!("Invalid subtraction, number too big (TODO: handle)")
                    })?,
                };

                cur = cur
                    .value_i64()
                    .checked_add(term)
                    .and_then(|val| sem::AtomVal::try_new(ty, val).ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Invalid summation, numbers too big (TODO: handle)")
                    })?;
            }

            Ok(ExprVal::Atom(cur))
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for Sign
    where
        Expr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                Sign::Plus(None) => gen!(ctx, ""),
                Sign::Plus(_) => gen!(ctx, " + "),
                Sign::Minus(_) => gen!(ctx, " - "),
            }
        }
    }

    lang_mixin!(Inspect, Sign, CommonMixin);

    impl<L> Gen<CommonMixin<'_, L>> for SumExpr
    where
        Expr: Gen<L>,
        Sign: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { terms, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            for (sign, term) in terms {
                gen!(ctx, "{}{}" % (sign, term))?;
            }
            gen!(ctx)
        }
    }

    lang_mixin!(Inspect, SumExpr, CommonMixin);
}
