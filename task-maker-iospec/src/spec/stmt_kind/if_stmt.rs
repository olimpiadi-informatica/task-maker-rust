pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct IfStmt {
        pub kw: syn::Token![if],
        pub cond: Expr,
        pub body: BracedBlock,
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for IfStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                cond: input.parse()?,
                body: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct IfStmt<T = Ir<MetaStmtKind>> {
        pub kw: syn::token::If,
        pub cond: Ir<Expr>,
        pub body: Ir<InnerBlock<T>>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::IfStmt> for IfStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::IfStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::IfStmt { kw, cond, body } = ast;

            Ok(Self {
                kw: kw.clone(),
                cond: cond.compile(env, dgns)?,
                body: (&body.content).compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<IfStmt<ast::MetaStmtKind>> for IfStmt {
        fn compile(
            input: &IfStmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let IfStmt { kw, cond, body } = input;

            Ok(Self {
                kw: kw.clone(),
                cond: cond.clone(),
                body: body.as_ref().compile(env, dgns)?,
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for IfStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            let cond = self.cond.eval(state, ctx)?.unwrap_value_i64();

            if cond != 0 {
                self.body.run(state, ctx)?
            }

            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for IfStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { cond, body, .. } = self;

            gen!(ctx, {
                "if {}:" % cond;
                ({ body });
            })
        }
    }
}
