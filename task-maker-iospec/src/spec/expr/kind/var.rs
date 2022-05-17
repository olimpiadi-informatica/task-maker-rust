pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct VarExpr {
        pub name: Name,
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct VarExpr {
        pub var: Ir<Var>,
        pub name: Ir<Name>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::VarExpr> for ExprKind {
        fn compile(ast: &ast::VarExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::VarExpr { name } = ast;

            let name = name.compile(env, dgns)?;
            let var = env.resolve(&name, dgns);

            Ok(ExprKind::Var(VarExpr { var, name }))
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Eval for VarExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            self.var.eval(state, ctx)
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for VarExpr
    where
        Name: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            self.name.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    lang_mixin!(Inspect, VarExpr, CommonMixin);
}
