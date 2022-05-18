pub mod kw {
    syn::custom_keyword!(assume);
    syn::custom_keyword!(assert);
}

pub mod ast {
    use super::kw;
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct CheckStmt {
        pub kw: CheckKw,
        pub cond: Expr,
        pub semi: syn::Token![;],
    }

    /// AST or either `assume` or `assert`.
    #[derive(Debug, Clone)]
    pub enum CheckKw {
        Assume(kw::assume),
        Assert(kw::assert),
    }
}

mod parse {
    use crate::ast;
    use syn::parse::*;

    impl Parse for ast::CheckStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                cond: input.parse()?,
                semi: input.parse()?,
            })
        }
    }

    impl Parse for ast::CheckKw {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();

            Ok(if la.peek(ast::kw::assume) {
                Self::Assume(input.parse()?)
            } else if la.peek(ast::kw::assert) {
                Self::Assert(input.parse()?)
            } else {
                unreachable!()
            })
        }
    }
}

pub mod ir {
    use crate::ast;
    use crate::ir::*;
    use crate::sem;

    pub type CheckKw = super::ast::CheckKw;

    #[derive(Debug)]
    pub struct CheckStmt {
        pub kw: CheckKw,
        pub cond: Ir<Expr>,
        pub semi: syn::Token![;],
    }

    impl CheckKw {
        pub fn to_stream(&self) -> sem::Stream {
            match self {
                ast::CheckKw::Assume(_) => sem::Stream::Input,
                ast::CheckKw::Assert(_) => sem::Stream::Output,
            }
        }
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::CheckStmt> for CheckStmt {
        fn compile(ast: &ast::CheckStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CheckStmt { kw, cond, semi } = ast;

            Ok(Self {
                kw: kw.clone(),
                cond: cond.compile(env, dgns)?,
                semi: semi.clone(),
            })
        }
    }
}

mod run {
    use crate::dgns::HasSpan;
    use crate::ir::*;
    use crate::run::*;

    impl Run for CheckStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            let res = self.cond.eval(state, ctx)?.unwrap_value_i64();
            if res == 0 {
                ctx.dgns.error(
                    "assumption violated",
                    vec![ctx.dgns.error_ann("condition is false", self.cond.span())],
                    vec![],
                );
                return Err(Stop::Done);
            }
            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for CheckStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { cond, .. } = self;

            // TODO: assume/assert
            gen!(ctx, {
                "check {};" % cond;
            })
        }
    }
}
