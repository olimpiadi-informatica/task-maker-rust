pub mod kw {
    syn::custom_keyword!(inputln);
    syn::custom_keyword!(outputln);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct IoStmt {
        pub kw: IoKw,
        pub body: BracedBlock,
    }

    /// AST or either `inputln` or `outputln`.

    #[derive(Debug, Clone)]
    pub enum IoKw {
        Input(kw::inputln),
        Output(kw::outputln),
    }
}

mod parse {
    use crate::ast;

    use syn::parse::*;

    impl Parse for ast::IoStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                body: input.parse()?,
            })
        }
    }

    impl Parse for ast::IoKw {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();

            Ok(if la.peek(ast::kw::inputln) {
                Self::Input(input.parse()?)
            } else if la.peek(ast::kw::outputln) {
                Self::Output(input.parse()?)
            } else {
                unreachable!()
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;
    use crate::sem;

    pub type IoKw = super::ast::IoKw;

    #[derive(Debug)]
    pub struct IoStmt<T = Ir<MetaStmtKind>> {
        pub kw: Ir<IoKw>,
        pub data_defs: Vec<Ir<DataDefExpr>>,
        pub stream: sem::Stream,
        pub body: Ir<InnerBlock<T>>,
    }

    impl IoKw {
        pub fn to_stream(&self) -> sem::Stream {
            match self {
                super::ast::IoKw::Input(_) => sem::Stream::Input,
                super::ast::IoKw::Output(_) => sem::Stream::Output,
            }
        }
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::IoStmt> for IoStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::IoStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::IoStmt { kw, body } = ast;
            let kw = Ir::new(kw.clone());

            let stream = kw.to_stream();

            if let Some(other_io) = &env.cur_io {
                dgns.error(
                    "nested I/O statements",
                    vec![
                        dgns.error_ann("nested I/O statement", kw.span()),
                        dgns.info_ann("inside this I/O statement", other_io.span()),
                    ],
                    vec![dgns.note_footer(
                        "I/O statements correspond to I/O lines and cannot be nested",
                    )],
                )
            }

            let block: Ir<InnerBlock<ast::MetaStmtKind>> =
                (&body.content).compile(&env.io(&kw), dgns)?;

            Ok(Self {
                kw,
                data_defs: block.data_defs.clone(),
                body: block,
                stream: stream,
            })
        }
    }

    impl CompileFrom<IoStmt<ast::MetaStmtKind>> for IoStmt {
        fn compile(
            input: &IoStmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let IoStmt {
                kw,
                data_defs,
                stream,
                body: block,
            } = input;
            Ok(Self {
                kw: kw.clone(),
                data_defs: data_defs.clone(),
                stream: stream.clone(),
                body: block.as_ref().compile(env, dgns)?,
            })
        }
    }
}

mod dgns {
    use syn::spanned::Spanned;

    use crate::ast;
    use crate::dgns::*;
    use crate::ir::*;

    impl HasSpan for IoKw {
        fn span(self: &Self) -> Span {
            match self {
                ast::IoKw::Input(kw) => kw.span(),
                ast::IoKw::Output(kw) => kw.span(),
            }
        }
    }
}

pub mod sem {
    #[derive(Debug, Clone, Copy)]
    pub enum Stream {
        Input,
        Output,
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for IoStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            self.body.run(state, ctx)
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;
    use crate::sem;

    pub struct Endl;

    struct InStream<T>(pub sem::Stream, pub T);
    pub struct InInput<T>(pub T);
    pub struct InOutput<T>(pub T);

    impl<L> Gen<CommonMixin<'_, L>> for IoStmt
    where
        InnerBlock: Gen<L>,
        for<'a> InStream<&'a Endl>: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { body: block, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            gen!(ctx, { (block, &InStream(self.stream, &Endl)) })
        }
    }

    impl<L, T> Gen<L> for InStream<&T>
    where
        for<'a> InInput<&'a T>: Gen<L>,
        for<'a> InOutput<&'a T>: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<L>) -> Result {
            match self.0 {
                sem::Stream::Input => InInput(self.1).gen(ctx),
                sem::Stream::Output => InOutput(self.1).gen(ctx),
            }
        }
    }

    impl Gen<Inspect> for IoStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            // TODO: input/output
            let mixin = CommonMixin(ctx.lang);
            let ctx = &mut ctx.with_lang(&mixin);
            gen!(ctx, {
                "ioln:";
                ({ self });
            })
        }
    }

    impl Gen<Inspect> for InInput<&Endl> {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx)
        }
    }

    impl Gen<Inspect> for InOutput<&Endl> {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx)
        }
    }
}
