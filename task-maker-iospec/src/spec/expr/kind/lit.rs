pub mod ast {
    #[derive(Debug, Clone)]
    pub struct IntLitExpr {
        pub token: syn::LitInt,
        pub value_i64: i64,
    }
}

pub mod ir {
    use crate::ir::*;
    use crate::sem;

    #[derive(Debug)]
    pub struct LitExpr {
        pub token: syn::LitInt,
        pub value: sem::AtomVal,
        pub ty: Ir<AtomTy>,
    }
}

mod compile {
    use std::str::FromStr;

    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;
    use crate::sem;

    impl CompileFrom<ast::IntLitExpr> for ExprKind {
        fn compile(
            ast: &ast::IntLitExpr,
            _env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::IntLitExpr { token, value_i64 } = ast;

            let suffix = token.suffix();
            let ty = if suffix.is_empty() {
                Some(sem::AtomTy::I32)
            } else {
                match sem::AtomTy::from_str(suffix) {
                    Ok(ty) => Some(ty),
                    Err(_) => {
                        dgns.error(
                            &format!("invalid literal suffix `{}`", suffix),
                            vec![dgns.error_ann("invalid suffix", token.span())],
                            vec![],
                        );

                        return Ok(Default::default());
                    }
                }
            };

            let value = match ty {
                Some(ty) => match sem::AtomVal::try_new(ty, *value_i64) {
                    Ok(value) => Some(value),
                    Err(_) => None,
                },
                _ => None,
            };

            Ok(if let Some(value) = value {
                ExprKind::Lit(LitExpr {
                    value,
                    ty: Ir::new(AtomTy {
                        sem: Some(value.ty()),
                        kind: AtomTyKind::Lit {
                            token: token.clone(),
                        },
                    }),
                    token: token.clone(),
                })
            } else {
                dgns.error(
                    &format!("invalid literal",),
                    vec![if ty.is_none() {
                        dgns.error_ann("invalid suffix", token.span())
                    } else {
                        dgns.error_ann("value outside range", token.span())
                    }],
                    vec![],
                );
                Default::default()
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Eval for LitExpr {
        fn eval<'a>(
            self: &Self,
            _state: &'a State,
            _ctx: &mut Context,
        ) -> Result<ExprVal<'a>, Stop> {
            Ok(ExprVal::Atom(self.value))
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for LitExpr {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let LitExpr { value, .. } = self;
            let value = value.value_i64();
            ctx.append(value)
        }
    }

    lang_mixin!(Inspect, LitExpr, CommonMixin);
}
