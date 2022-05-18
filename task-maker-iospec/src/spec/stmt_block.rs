pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct BracedBlock {
        pub brace: syn::token::Brace,
        pub content: BlockContent,
    }

    #[derive(Debug, Clone)]
    pub struct BlockContent {
        pub stmts: Vec<Stmt>,
    }
}

mod parse {
    use crate::ast;

    use syn::parse::*;

    impl Parse for ast::BracedBlock {
        fn parse(input: ParseStream) -> Result<Self> {
            let content;
            Ok(Self {
                brace: syn::braced!(content in input),
                content: content.parse()?,
            })
        }
    }

    impl Parse for ast::BlockContent {
        fn parse(input: ParseStream) -> Result<Self> {
            let mut stmts = vec![];
            while !input.is_empty() {
                stmts.push(input.parse()?);
            }
            Ok(Self { stmts })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct OuterBlock<T = Ir<MetaStmtKind>> {
        pub inner: Ir<InnerBlock<T>>,
        pub data_defs: Vec<Ir<DataDefExpr>>,
        pub decls: Vec<Ir<DataVar>>,
    }

    #[derive(Debug)]
    pub struct InnerBlock<T = Ir<MetaStmtKind>> {
        pub stmts: Vec<Ir<Stmt<T>>>,
        pub data_defs: Vec<Ir<DataDefExpr>>,
        pub calls: Vec<Ir<CallMetaStmt>>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl<T> OuterBlock<T> {
        pub fn new(data_defs: Vec<Ir<DataDefExpr>>, block: InnerBlock<T>) -> Ir<Self> {
            Ir::new(Self {
                decls: data_defs
                    .iter()
                    .filter_map(|expr| expr.var.as_ref())
                    .cloned()
                    .collect(),
                data_defs,
                inner: Ir::new(block),
            })
        }
    }

    impl CompileFrom<ast::BlockContent> for InnerBlock<ast::MetaStmtKind> {
        fn compile(
            ast: &ast::BlockContent,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            // Compile non-meta statements first, as they can change the environment
            let mut stmts = Vec::new();
            let env = &mut env.clone();

            for stmt in ast.stmts.iter() {
                let has_cfg_false = stmt
                    .attrs
                    .iter()
                    .filter_map(|attr| match &attr.kind {
                        ast::StmtAttrKind::Cfg(attr) => Some(attr),
                        _ => None,
                    })
                    .map(|attr| -> Result<bool> { (&attr.expr).compile(env, dgns) })
                    .find_map(|attr| match attr {
                        Ok(true) => None,
                        Ok(false) => Some(Ok(())),
                        Err(err) => Some(Err(err)),
                    })
                    .transpose()?
                    .is_some();

                if has_cfg_false {
                    continue;
                }

                let stmt: Ir<Stmt<ast::MetaStmtKind>> = stmt.compile(env, dgns)?;
                for expr in stmt.data_defs.iter() {
                    env.declare_expr(expr, dgns)
                }
                stmts.push(stmt)
            }

            Ok(Self {
                data_defs: stmts
                    .iter()
                    .flat_map(|s| s.data_defs.iter())
                    .cloned()
                    .collect(),
                calls: vec![],
                stmts,
            })
        }
    }

    impl CompileFrom<InnerBlock<ast::MetaStmtKind>> for InnerBlock {
        fn compile(
            input: &InnerBlock<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let InnerBlock {
                calls: _,
                data_defs,
                stmts,
            } = input;

            let stmts: Vec<Ir<Stmt>> = stmts
                .iter()
                .map(|stmt| stmt.as_ref().compile(env, dgns))
                .collect::<Result<_>>()?;

            Ok(Self {
                calls: stmts.iter().flat_map(|s| s.calls.iter()).cloned().collect(),
                data_defs: data_defs.clone(),
                stmts,
            })
        }
    }

    impl CompileFrom<OuterBlock<ast::MetaStmtKind>> for OuterBlock {
        fn compile(
            input: &OuterBlock<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let OuterBlock {
                inner: block,
                decls,
                data_defs,
            } = input;

            let env = &mut env.clone();

            for expr in data_defs.iter() {
                env.declare_expr(expr, dgns)
            }

            Ok(Self {
                inner: block.as_ref().compile(env, dgns)?,
                decls: decls.clone(),
                data_defs: data_defs.clone(),
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for OuterBlock {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            self.inner.run(state, ctx)
        }
    }

    impl Run for InnerBlock {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            for step in self.stmts.iter() {
                step.run(state, ctx)?;
            }
            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for OuterBlock
    where
        InnerBlock: Gen<L>,
        DataVar: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self {
                inner: block,
                decls,
                ..
            } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);

            if !decls.is_empty() {
                for decl in decls.iter() {
                    gen!(ctx, decl)?
                }
                gen!(ctx, {
                    ();
                })?
            }

            gen!(ctx, block)
        }
    }

    impl Gen<Inspect> for OuterBlock {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx, {
                "<<block>>";
                ();
            })?;

            let mixin = CommonMixin(&Inspect);
            let ctx = &mut ctx.with_lang(&mixin);
            gen!(ctx, { self })
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for InnerBlock
    where
        Stmt: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            for stmt in self.stmts.iter() {
                gen!(ctx, stmt)?
            }
            gen!(ctx)
        }
    }

    lang_mixin!(Inspect, InnerBlock, CommonMixin);
}
