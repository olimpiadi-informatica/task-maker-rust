pub mod kw {
    syn::custom_keyword!(item);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct ItemStmt {
        pub kw: crate::ast::kw::item,
        pub expr: Expr,
        pub colon: syn::Token![:],
        pub ty: Name,
        pub semi: syn::Token![;],
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for ItemStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                expr: input.parse()?,
                colon: input.parse()?,
                ty: input.parse()?,
                semi: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use std::marker::PhantomData;

    use crate::ir::*;
    use crate::sem;

    #[derive(Debug)]
    pub struct ItemStmt<T = Ir<MetaStmtKind>> {
        pub kw: super::kw::item,
        pub colon: syn::token::Colon,
        pub expr: Ir<DataDefExpr>,
        pub ty: Ir<AtomTy>,
        pub io: Option<Ir<IoKw>>,
        pub stream: Option<sem::Stream>,
        pub semi: syn::token::Semi,
        pub phase: PhantomData<T>,
    }
}

mod compile {
    use std::marker::PhantomData;

    use syn::spanned::Spanned;

    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::ItemStmt> for ItemStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::ItemStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::ItemStmt {
                kw,
                expr,
                colon,
                ty,
                semi,
            } = ast;

            if env.cur_io.is_none() {
                dgns.error(
                    "`item` statement outside I/O block",
                    vec![dgns.error_ann("must be inside an I/O block", kw.span())],
                    vec![dgns.note_footer(
                        "`item` statements must occur inside a `inputln` or `outputln` block.",
                    )],
                )
            }

            let ty: Ir<AtomTy> = ty.compile(env, dgns)?;

            Ok(Self {
                kw: kw.clone(),
                colon: colon.clone(),
                expr: expr.compile(&env.data_env(&ty), dgns)?,
                ty,
                io: env.cur_io.clone(),
                stream: env.cur_io.as_ref().map(|io| io.to_stream()),
                semi: semi.clone(),
                phase: PhantomData,
            })
        }
    }

    impl CompileFrom<ItemStmt<ast::MetaStmtKind>> for ItemStmt {
        fn compile(
            input: &ItemStmt<ast::MetaStmtKind>,
            _env: &Env,
            _dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ItemStmt {
                kw,
                colon,
                expr,
                ty,
                io,
                stream,
                semi,
                phase: _,
            } = input;

            Ok(Self {
                kw: kw.clone(),
                colon: colon.clone(),
                expr: expr.clone(),
                ty: ty.clone(),
                io: io.clone(),
                stream: stream.clone(),
                semi: semi.clone(),
                phase: PhantomData,
            })
        }
    }
}

mod run {
    use crate::dgns::*;
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;
    use crate::sem;

    impl Run for ItemStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            if let Some(var) = &self.expr.var {
                state.decl(var);
            }

            let io_source = match self.io.as_ref().unwrap().as_ref() {
                IoKw::Input(_) => Ok(&mut ctx.input_source),
                IoKw::Output(_) => (&mut ctx.output_source).as_mut().ok_or_else(|| {
                    if !ctx.input_source.check_eof() {
                        ctx.dgns.error(
                            "expected EOF",
                            vec![ctx
                                .dgns
                                .info_ann("reached output data here", self.try_span().unwrap())],
                            vec![ctx.dgns.note_footer(
                            "expecting output data at this point, so no more input can be checked",
                        )],
                        );
                    }
                    Stop::Done
                }),
            }?;

            let val = io_source
                .next_atom(&self.ty.sem.unwrap())
                .map_err(|e| {
                    ctx.dgns.error(
                        &format!("invalid literal: `{}`", &e.to_string()),
                        vec![ctx
                            .dgns
                            .info_ann("when reading this", self.try_span().unwrap())],
                        vec![],
                    );
                    anyhow::anyhow!("invalid I/O file")
                })?
                .ok_or_else(|| {
                    ctx.dgns.error(
                        "premature EOF",
                        vec![ctx
                            .dgns
                            .info_ann("when reading this", self.try_span().unwrap())],
                        vec![],
                    );
                    anyhow::anyhow!("invalid I/O file")
                })?;

            let val = sem::AtomVal::try_new(self.ty.sem.unwrap(), val).map_err(|_| {
                ctx.dgns.error(
                    "invalid atomic value",
                    vec![ctx
                        .dgns
                        .info_ann("when reading this", self.try_span().unwrap())],
                    vec![],
                );
                anyhow::anyhow!("invalid I/O file")
            })?;

            let atom = self.expr.eval_mut(state, ctx)?;

            match atom {
                ExprValMut::Atom(atom) => atom.set(val),
                _ => unreachable!(),
            }

            Ok(())
        }
    }
}

mod dgns {
    use crate::dgns::*;
    use crate::ir::*;

    impl TryHasSpan for ItemStmt {
        fn try_span(self: &Self) -> Option<Span> {
            self.expr.try_span()
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;
    use crate::sem;

    impl<L> Gen<CommonMixin<'_, L>> for ItemStmt
    where
        for<'a> InInput<&'a ItemStmt>: Gen<L>,
        for<'a> InOutput<&'a ItemStmt>: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self.stream {
                Some(sem::Stream::Input) => InInput(self).gen(&mut ctx.with_lang(ctx.lang.0)),
                Some(sem::Stream::Output) => InOutput(self).gen(&mut ctx.with_lang(ctx.lang.0)),
                None => gen!(ctx, {
                    "<<item without I/O>>";
                }),
            }
        }
    }

    impl Gen<Inspect> for ItemStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { expr, ty, .. } = self;
            gen!(ctx, {
                "item {}: {};" % (expr, ty);
            })
        }
    }
}
