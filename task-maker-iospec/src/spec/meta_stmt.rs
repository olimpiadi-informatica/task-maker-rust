pub mod ast {
    use syn::Token;

    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct MetaStmt {
        pub at_sign: Token![@],
        pub kind: MetaStmtKind,
    }

    #[derive(Debug, Clone)]
    pub enum MetaStmtKind {
        Set(SetMetaStmt),
        Call(CallMetaStmt),
        Resize(ResizeMetaStmt),
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for MetaStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                at_sign: input.parse()?,
                kind: input.parse()?,
            })
        }
    }

    impl Parse for MetaStmtKind {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(kw::set) {
                Self::Set(input.parse()?)
            } else if la.peek(kw::call) {
                Self::Call(input.parse()?)
            } else if la.peek(kw::resize) {
                Self::Resize(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    /// IR of a `@`-statement.
    #[derive(Debug)]
    pub struct MetaStmt<T = Ir<MetaStmtKind>> {
        pub at_sign: syn::token::At,
        pub kind: T,
    }

    #[derive(Debug)]
    pub enum MetaStmtKind {
        Set(Ir<SetMetaStmt>),
        Call(Ir<CallMetaStmt>),
        Resize(Ir<ResizeMetaStmt>),
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::MetaStmt> for MetaStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::MetaStmt, _env: &Env, _dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::MetaStmt { at_sign, kind } = ast;
            Ok(Self {
                at_sign: at_sign.clone(),
                kind: kind.clone(),
            })
        }
    }
    impl CompileFrom<MetaStmt<ast::MetaStmtKind>> for MetaStmt {
        fn compile(
            input: &MetaStmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let MetaStmt { at_sign, kind } = input;
            Ok(Self {
                at_sign: at_sign.clone(),
                kind: kind.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::MetaStmtKind> for MetaStmtKind {
        fn compile(
            ast: &ast::MetaStmtKind,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match ast {
                ast::MetaStmtKind::Set(stmt) => Self::Set(stmt.compile(env, dgns)?),
                ast::MetaStmtKind::Call(stmt) => Self::Call(stmt.compile(env, dgns)?),
                ast::MetaStmtKind::Resize(stmt) => Self::Resize(stmt.compile(env, dgns)?),
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for MetaStmt {
        fn run(self: &Self, _state: &mut State, _ctx: &mut Context) -> Result<(), Stop> {
            // TODO: we should run meta statements to check they are correct,
            // even though they should have no effect on the I/O validation itself.
            Ok(())
        }
    }
}

mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for MetaStmt
    where
        MetaStmtKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { kind, .. } = self;
            kind.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    impl Gen<Inspect> for MetaStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let ctx = &mut ctx.with_lang(&CommonMixin(&Inspect));
            gen!(ctx, {
                "@{}" % self;
            })
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for MetaStmtKind
    where
        SetMetaStmt: Gen<L>,
        CallMetaStmt: Gen<L>,
        ResizeMetaStmt: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            match self {
                Self::Set(stmt) => stmt.gen(ctx),
                Self::Call(stmt) => stmt.gen(ctx),
                Self::Resize(stmt) => stmt.gen(ctx),
            }
        }
    }

    lang_mixin!(Inspect, MetaStmtKind, CommonMixin);
}
