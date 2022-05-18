pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct Stmt {
        pub attrs: Vec<StmtAttr>,
        pub kind: StmtKind,
    }

    #[derive(Debug, Clone)]
    pub enum StmtKind {
        Io(IoStmt),
        Check(CheckStmt),
        Item(ItemStmt),
        For(ForStmt),
        If(IfStmt),
        Block(BlockStmt),
        Meta(MetaStmt),
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for Stmt {
        fn parse(input: ParseStream) -> Result<Self> {
            let mut attrs = Vec::<StmtAttr>::new();

            while input.peek(syn::Token![#]) {
                attrs.push(input.parse()?)
            }

            Ok(Self {
                attrs,
                kind: input.parse()?,
            })
        }
    }

    impl Parse for StmtKind {
        fn parse(input: ParseStream) -> Result<Self> {
            use StmtKind::*;

            let la = input.lookahead1();

            Ok(if la.peek(kw::inputln) || la.peek(kw::outputln) {
                Io(input.parse()?)
            } else if la.peek(kw::assume) || la.peek(kw::assert) {
                Check(input.parse()?)
            } else if la.peek(kw::item) {
                Item(input.parse()?)
            } else if la.peek(syn::Token![for]) {
                For(input.parse()?)
            } else if la.peek(syn::Token![if]) {
                If(input.parse()?)
            } else if la.peek(syn::token::Brace) {
                Block(input.parse()?)
            } else if la.peek(syn::Token![@]) {
                Meta(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    /// IR of a statement.
    /// Type parameter `T` depends on the phase of compilation.
    #[derive(Debug)]
    pub struct Stmt<T = Ir<MetaStmtKind>> {
        pub attrs: Vec<Ir<StmtAttr>>,
        pub kind: Ir<StmtKind<T>>,
        /// Data expressions defined inside this statement
        pub data_defs: Vec<Ir<DataDefExpr>>,
        pub calls: Vec<Ir<CallMetaStmt>>,
        pub allocs: Vec<DataExprAlloc>,
    }

    #[derive(Debug)]
    pub enum StmtKind<T = Ir<MetaStmtKind>> {
        Io(Ir<IoStmt<T>>),
        Item(Ir<ItemStmt<T>>),
        Check(Ir<CheckStmt>),
        For(Ir<ForStmt<T>>),
        If(Ir<IfStmt<T>>),
        Block(Ir<BlockStmt<T>>),
        Meta(Ir<MetaStmt<T>>),
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::StmtKind> for StmtKind<ast::MetaStmtKind> {
        fn compile(ast: &ast::StmtKind, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            Ok(match ast {
                ast::StmtKind::Io(stmt) => Self::Io(stmt.compile(env, dgns)?),
                ast::StmtKind::Item(stmt) => Self::Item(stmt.compile(env, dgns)?),
                ast::StmtKind::Check(stmt) => Self::Check(stmt.compile(env, dgns)?),
                ast::StmtKind::For(stmt) => Self::For(stmt.compile(env, dgns)?),
                ast::StmtKind::If(stmt) => Self::If(stmt.compile(env, dgns)?),
                ast::StmtKind::Block(stmt) => Self::Block(stmt.compile(env, dgns)?),
                ast::StmtKind::Meta(stmt) => Self::Meta(stmt.compile(env, dgns)?),
            })
        }
    }

    impl CompileFrom<StmtKind<ast::MetaStmtKind>> for StmtKind {
        fn compile(
            input: &StmtKind<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match input {
                StmtKind::Io(stmt) => Self::Io(stmt.as_ref().compile(env, dgns)?),
                StmtKind::Item(stmt) => Self::Item(stmt.as_ref().compile(env, dgns)?),
                StmtKind::Check(stmt) => Self::Check(stmt.clone()),
                StmtKind::For(stmt) => Self::For(stmt.as_ref().compile(env, dgns)?),
                StmtKind::If(stmt) => Self::If(stmt.as_ref().compile(env, dgns)?),
                StmtKind::Block(stmt) => Self::Block(stmt.as_ref().compile(env, dgns)?),
                StmtKind::Meta(stmt) => Self::Meta(stmt.as_ref().compile(env, dgns)?),
            })
        }
    }

    impl CompileFrom<ast::Stmt> for Stmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::Stmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::Stmt { attrs, kind } = ast;

            let kind: Ir<StmtKind<ast::MetaStmtKind>> = kind.compile(env, dgns)?;

            let data_defs = match kind.as_ref() {
                StmtKind::For(step) => step.data_defs.clone(),
                StmtKind::If(step) => step.body.data_defs.clone(),
                StmtKind::Io(step) => step.data_defs.clone(),
                StmtKind::Item(step) => vec![step.expr.clone()],
                _ => Vec::new(),
            };

            let mut env = env.clone();

            for expr in data_defs.iter() {
                env.declare_expr(expr, dgns)
            }

            Ok(Stmt {
                attrs: attrs.iter().cloned().map(Ir::new).collect(),
                data_defs,
                calls: vec![],
                allocs: match kind.as_ref() {
                    StmtKind::For(step) => step.allocs.clone(),
                    _ => Vec::new(),
                },
                kind,
            })
        }
    }

    impl CompileFrom<Stmt<ast::MetaStmtKind>> for Stmt {
        fn compile(
            input: &Stmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let Stmt {
                attrs,
                kind,
                data_defs,
                calls: _,
                allocs,
            } = input;

            let kind: Ir<StmtKind> = kind.as_ref().compile(env, dgns)?;

            let calls = match kind.as_ref() {
                StmtKind::For(stmt) => stmt.body.inner.calls.clone(),
                StmtKind::If(stmt) => stmt.body.calls.clone(),
                StmtKind::Io(stmt) => stmt.body.calls.clone(),
                StmtKind::Meta(stmt) => match stmt.kind.as_ref() {
                    MetaStmtKind::Call(stmt) => vec![stmt.clone()],
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };

            Ok(Self {
                attrs: attrs.clone(),
                kind,
                data_defs: data_defs.clone(),
                calls: calls.clone(),
                allocs: allocs.clone(),
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for Stmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            match self.kind.as_ref() {
                StmtKind::Io(stmt) => stmt.run(state, ctx)?,
                StmtKind::Item(stmt) => stmt.run(state, ctx)?,
                StmtKind::Check(stmt) => stmt.run(state, ctx)?,
                StmtKind::For(stmt) => stmt.run(state, ctx)?,
                StmtKind::If(stmt) => stmt.run(state, ctx)?,
                StmtKind::Block(stmt) => stmt.run(state, ctx)?,
                StmtKind::Meta(stmt) => stmt.run(state, ctx)?,
            }
            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for Stmt
    where
        StmtAttr: Gen<L>,
        DataExprAlloc: Gen<L>,
        StmtKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self {
                allocs,
                attrs,
                kind,
                ..
            } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);

            for attr in attrs.iter() {
                gen!(ctx, attr)?
            }

            for alloc in allocs.iter() {
                gen!(ctx, alloc)?
            }

            gen!(ctx, kind)
        }
    }

    lang_mixin!(Inspect, Stmt, CommonMixin);

    impl<L> Gen<CommonMixin<'_, L>> for StmtKind
    where
        IoStmt: Gen<L>,
        ItemStmt: Gen<L>,
        CheckStmt: Gen<L>,
        ForStmt: Gen<L>,
        IfStmt: Gen<L>,
        BlockStmt: Gen<L>,
        MetaStmt: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                Self::Io(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Item(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Check(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::For(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::If(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Block(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
                Self::Meta(stmt) => stmt.gen(&mut ctx.with_lang(ctx.lang.0)),
            }
        }
    }

    lang_mixin!(Inspect, StmtKind, CommonMixin);
}
