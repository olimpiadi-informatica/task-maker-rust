pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct SubscriptExpr {
        pub array: Box<Expr>,
        pub bracket: syn::token::Bracket,
        pub index: Box<Expr>,
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct SubscriptExpr {
        pub array: Ir<Expr>,
        pub bracket: syn::token::Bracket,
        pub index: Ir<Expr>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::SubscriptExpr> for ExprKind {
        fn compile(
            ast: &ast::SubscriptExpr,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::SubscriptExpr {
                array,
                bracket,
                index,
            } = ast;

            Ok(ExprKind::Subscript(SubscriptExpr {
                array: array.as_ref().compile(env, dgns)?,
                index: index.as_ref().compile(env, dgns)?,
                bracket: bracket.clone(),
            }))
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Eval for SubscriptExpr {
        fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop> {
            let index = self.index.eval(state, ctx)?.unwrap_value_i64() as usize;

            Ok(
                match (self.array.ty.as_ref(), self.array.eval(state, ctx)?) {
                    (ExprTy::Array { item, .. }, ExprVal::Array(aggr)) => {
                        match (item.as_ref(), aggr) {
                            (ExprTy::Atom { atom_ty }, ArrayVal::AtomArray(array)) => {
                                ExprVal::Atom(
                                    array
                                        .at(index)
                                        .get(atom_ty.sem.unwrap())
                                        .expect("TODO: handle empty"),
                                )
                            }
                            (_, ArrayVal::AggrArray(array)) => ExprVal::Array(&array[index]),
                            _ => todo!(),
                        }
                    }
                    _ => todo!(),
                },
            )
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for SubscriptExpr
    where
        Expr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { array, index, .. } = self;
            let ctx = &mut ctx.with_lang(ctx.lang.0);
            gen!(ctx, "{}[{}]" % (array, index))
        }
    }

    lang_mixin!(Inspect, SubscriptExpr, CommonMixin);
}
