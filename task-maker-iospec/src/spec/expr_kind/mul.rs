pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct MulExpr {
        pub factors: syn::punctuated::Punctuated<Expr, syn::Token![*]>,
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct MulExpr {
        pub factors: Vec<Ir<Expr>>,
        pub ops: Vec<syn::Token![*]>,
        pub ty: Ir<AtomTy>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::MulExpr> for ExprKind {
        fn compile(ast: &ast::MulExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::MulExpr { factors } = ast;
            let (factors, ops) = unzip_punctuated(factors.clone());
            let factors: Vec<Ir<Expr>> = factors
                .iter()
                .map(|f| f.compile(env, dgns))
                .collect::<Result<_>>()?;

            let ty: Option<Ir<AtomTy>> = ExprList(&factors).analyze(dgns);

            Ok(match ty {
                Some(ty) => ExprKind::Mul(MulExpr {
                    ty: ty.clone(),
                    factors,
                    ops,
                }),
                _ => Default::default(),
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;
    use crate::sem;

    impl Eval for MulExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            let ty = self.ty.sem.unwrap();
            let mut cur = sem::AtomVal::new(ty, 1);

            for factor in &self.factors {
                let factor = factor.eval(state, ctx)?.unwrap_value_i64();

                cur = cur
                    .value_i64()
                    .checked_mul(factor)
                    .and_then(|val| sem::AtomVal::try_new(ty, val).ok())
                    .ok_or_else(|| anyhow::anyhow!("mul too big (TODO: handle this)"))?;
            }
            Ok(ExprVal::Atom(cur))
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for MulExpr
    where
        Expr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { factors, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            gen!(ctx, "{}" % (&Punctuated(factors.to_vec(), " * ")))
        }
    }

    lang_mixin!(Inspect, MulExpr, CommonMixin);
}
