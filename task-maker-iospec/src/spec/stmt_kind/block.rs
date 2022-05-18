//! Groups zero or more statements

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct BlockStmt {
        pub body: BracedBlock,
    }
}

mod parse {
    use crate::ast;
    use syn::parse::*;

    impl Parse for ast::BlockStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                body: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct BlockStmt<T = Ir<MetaStmtKind>> {
        pub brace: syn::token::Brace,
        pub block: InnerBlock<T>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::BlockStmt> for BlockStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::BlockStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::BlockStmt { body } = ast;
            let ast::BracedBlock {
                brace,
                content: block,
            } = body;

            Ok(Self {
                brace: brace.clone(),
                block: block.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<BlockStmt<ast::MetaStmtKind>> for BlockStmt {
        fn compile(
            input: &BlockStmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let BlockStmt { brace, block } = input;

            Ok(Self {
                brace: brace.clone(),
                block: block.compile(env, dgns)?,
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for BlockStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            self.block.run(state, ctx)
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for BlockStmt
    where
        InnerBlock: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { block, .. } = self;
            gen!(&mut ctx.with_lang(ctx.lang.0), block)
        }
    }

    impl Gen<Inspect> for BlockStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { block, .. } = self;
            gen!(ctx, {
                ({ block });
            })
        }
    }
}
