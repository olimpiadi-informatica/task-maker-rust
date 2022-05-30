pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct ParenExpr {
        pub paren: syn::token::Paren,
        pub inner: Box<Expr>,
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct ParenExpr {
        pub paren: syn::token::Paren,
        pub inner: Ir<Expr>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::ParenExpr> for ExprKind {
        fn compile(ast: &ast::ParenExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::ParenExpr { paren, inner } = ast;

            Ok(ExprKind::Paren(ParenExpr {
                paren: paren.clone(),
                inner: inner.as_ref().compile(env, dgns)?,
            }))
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Eval for ParenExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            self.inner.eval(state, ctx)
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for ParenExpr
    where
        Expr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { inner, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            gen!(ctx, "({})" % inner)
        }
    }

    lang_mixin!(Inspect, ParenExpr, CommonMixin);
}
