pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct Spec {
        pub attrs: Vec<SpecAttr>,
        pub block: BlockContent,
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;
    use syn::Token;

    impl Parse for Spec {
        fn parse(input: ParseStream) -> Result<Self> {
            let mut attrs = Vec::<SpecAttr>::new();

            while input.peek(Token![#]) && input.peek2(Token![!]) {
                attrs.push(input.parse()?)
            }

            Ok(Self {
                attrs,
                block: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    pub struct Spec {
        pub attrs: Vec<Ir<SpecAttr>>,
        pub main: Ir<OuterBlock>,
    }

    pub struct Template<T>(pub T);
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::Spec> for Spec {
        fn compile(ast: &ast::Spec, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::Spec { attrs, block } = ast;
            let attrs = attrs.into_iter().cloned().map(Ir::new).collect();
            let block: InnerBlock<ast::MetaStmtKind> = block.compile(env, dgns)?;
            let main = OuterBlock::new(block.data_defs.clone(), block);
            Ok(Self {
                attrs,
                main: main.as_ref().compile(env, dgns)?,
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::run::*;

    impl Run for Spec {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            self.main.run(state, ctx)?;

            let token = ctx
                .input_source
                .next_token()
                .map_err(|_| Stop::Error(anyhow::anyhow!("cannot read from input file")))?;

            if !token.is_empty() {
                ctx.dgns.error(
                    &format!(
                        "expected EOF in input file, got {}",
                        String::from_utf8_lossy(&token)
                    ),
                    vec![],
                    vec![ctx.dgns.note_footer("reached end of spec")],
                )
            }

            if let Some(output_source) = &mut ctx.output_source {
                if !output_source.check_eof() {
                    ctx.dgns.error(
                        "expected EOF in output file",
                        vec![],
                        vec![ctx.dgns.note_footer("reached end of spec")],
                    )
                }
            }

            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for Spec {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { attrs, main, .. } = self;

            gen!(ctx, {
                "<<spec>>";
                ();
            })?;

            for attr in attrs {
                gen!(ctx, attr)?;
            }

            gen!(ctx, {
                (main);
                ();
            })
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for InFunDecl<&Spec>
    where
        for<'a> InFunDecl<&'a CallMetaStmt>: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let ctx = &mut ctx.with_lang(ctx.lang.0);

            let calls = &self.0.main.inner.calls;
            if !calls.is_empty() {
                gen!(ctx, {
                    ();
                })?;
                for call in calls.iter() {
                    gen!(ctx, (&InFunDecl(call.as_ref())))?;
                }
            }

            gen!(ctx)
        }
    }
}
