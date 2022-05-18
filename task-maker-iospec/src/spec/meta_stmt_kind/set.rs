pub mod kw {
    syn::custom_keyword!(set);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct SetMetaStmt {
        pub kw: kw::set,
        pub lexpr: Expr,
        pub eq: syn::Token![=],
        pub rexpr: Expr,
        pub semi: syn::Token![;],
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for SetMetaStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                lexpr: input.parse()?,
                eq: input.parse()?,
                rexpr: input.parse()?,
                semi: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ast;
    use crate::ir::*;

    #[derive(Debug)]
    pub struct SetMetaStmt {
        pub kw: ast::kw::set,
        pub lexpr: Expr,
        pub eq: syn::Token![=],
        pub rexpr: Expr,
        pub semi: syn::Token![;],
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::SetMetaStmt> for SetMetaStmt {
        fn compile(
            ast: &ast::SetMetaStmt,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::SetMetaStmt {
                kw,
                lexpr,
                eq,
                rexpr,
                semi,
            } = ast;
            Ok(Self {
                kw: kw.clone(),
                lexpr: lexpr.compile(env, dgns)?,
                eq: eq.clone(),
                rexpr: rexpr.compile(env, dgns)?,
                semi: semi.clone(),
            })
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for SetMetaStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { lexpr, rexpr, .. } = self;
            gen!(ctx, "set {} = {};" % (lexpr, rexpr))
        }
    }
}
