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
            })
        }
    }
}

mod run;

pub mod gen;
