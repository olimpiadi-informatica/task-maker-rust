pub mod ir {
    use crate::ir::*;

    /// IR of the definition a value (either atomic or aggregate) in input/output data.
    /// E.g., `A`, `A[i]` and `A[i][j]`, in `item A[i][j]: n32;`.
    #[derive(Debug, Clone)]
    pub struct DataDefExpr {
        pub kind: DataDefExprKind,
        pub ty: Ir<ExprTy>,
        pub root_var: Ir<DataVar>,
        pub var: Option<Ir<DataVar>>,
        pub alloc: Option<AllocInfo>,
    }

    #[derive(Debug, Clone)]
    pub enum DataDefExprKind {
        Var {
            var: Ir<DataVar>,
        },
        Subscript {
            array: Ir<DataDefExpr>,
            bracket: syn::token::Bracket,
            index: Ir<Expr>,
        },
        Err,
    }

    impl Default for DataDefExprKind {
        fn default() -> Self {
            Self::Err
        }
    }

    #[derive(Debug, Clone)]
    pub struct DataExprAlloc {
        pub expr: Ir<DataDefExpr>,
        pub info: AllocInfo,
    }

    #[derive(Debug, Clone)]
    pub struct AllocInfo {
        pub item_ty: Ir<ExprTy>,
        pub size: Ir<Expr>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::Expr, DataDefEnv> for DataDefExpr {
        fn compile(
            ast: &ast::Expr,
            env: &DataDefEnv,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let kind: DataDefExprKind = ast.compile(env, dgns)?;

            Ok(Self {
                ty: env.ty.clone(),
                root_var: match &kind {
                    DataDefExprKind::Var { var } => var.clone(),
                    DataDefExprKind::Subscript { array, .. } => array.root_var.clone(),
                    DataDefExprKind::Err => Err(CompileStop)?,
                },
                var: match &kind {
                    DataDefExprKind::Var { var } => Some(var.clone()),
                    _ => None,
                },
                alloc: match env.ty.as_ref() {
                    ExprTy::Array { item, range } => Some(AllocInfo {
                        item_ty: item.clone(),
                        size: range.bound.val.clone(),
                    }),
                    _ => None,
                },
                kind,
            })
        }
    }

    impl CompileFrom<ast::Expr, DataDefEnv> for DataDefExprKind {
        fn compile(
            ast: &ast::Expr,
            env: &DataDefEnv,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match ast {
                ast::Expr::Var(expr) => expr.compile(env, dgns)?,
                ast::Expr::Subscript(expr) => expr.compile(env, dgns)?,
                other => {
                    dgns.error(
                        "invalid expression in definition",
                        vec![dgns.error_ann("invalid expression", HasSpan::span(other))],
                        vec![dgns.note_footer("only variables and subscripts are allowed")],
                    );
                    Default::default()
                }
            })
        }
    }

    impl CompileFrom<ast::VarExpr, DataDefEnv> for DataDefExprKind {
        fn compile(
            ast: &ast::VarExpr,
            env: &DataDefEnv,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::VarExpr { name } = ast;

            Ok(DataDefExprKind::Var {
                var: name.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::SubscriptExpr, DataDefEnv> for DataDefExprKind {
        fn compile(
            ast: &ast::SubscriptExpr,
            env: &DataDefEnv,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::SubscriptExpr {
                array,
                bracket,
                index,
            } = ast;

            let index: Ir<Expr> = index.as_ref().compile(env.outer.as_ref(), dgns)?;

            Ok(match env.loc.as_ref() {
                Loc::For {
                    range: expected_range,
                    parent,
                } => match &index.kind {
                    ExprKind::Var(VarExpr { name, var }) => match &var.kind {
                        VarKind::Index {
                            range: actual_range,
                        } => {
                            if Ir::same(&expected_range, &actual_range) {
                                Self::Subscript {
                                    array: array.as_ref().compile(
                                        &DataDefEnv {
                                            outer: env.outer.clone(),
                                            ty: Ir::new(ExprTy::Array {
                                                item: env.ty.clone(),
                                                range: expected_range.clone(),
                                            }),
                                            loc: parent.clone(),
                                        },
                                        dgns,
                                    )?,
                                    bracket: bracket.clone(),
                                    index,
                                }
                            } else {
                                let message = format!("subscript must match an enclosing `for` index, expecting `{}`, got `{}`", quote_hir(expected_range.index.as_ref()), quote_hir(name.as_ref()));
                                dgns.error(
                                    &message,
                                    vec![
                                        dgns.error_ann(
                                            "does not match enclosing `for` index",
                                            name.span(),
                                        ),
                                        dgns.info_ann(
                                            "must match this index",
                                            expected_range.index.span(),
                                        ),
                                    ],
                                    vec![],
                                );
                                Default::default()
                            }
                        }
                        _ => {
                            let message = format!("subscript must match an enclosing `for` index, expecting `{}`, got `{}`", quote_hir(expected_range.index.as_ref()), quote_hir(name.as_ref()));
                            dgns.error(
                                &message,
                                vec![
                                    dgns.error_ann(
                                        "does not match enclosing `for` index",
                                        name.span(),
                                    ),
                                    dgns.info_ann(
                                        "must match this index",
                                        expected_range.index.span(),
                                    ),
                                ],
                                vec![],
                            );
                            Default::default()
                        }
                    },
                    _ => {
                        let message = format!(
                                    "subscript must match an enclosing `for` index, expecting `{}`, got an expression",
                                    quote_hir(expected_range.index.as_ref()),
                                );
                        dgns.error(
                            &message,
                            vec![
                                dgns.error_ann(
                                    "complex expressions not allowed here",
                                    index.span(),
                                ),
                                dgns.info_ann("must match this index", expected_range.index.span()),
                            ],
                            vec![],
                        );
                        Default::default()
                    }
                },
                _ => {
                    dgns.error(
                        "subscript must match an enclosing `for` index, but none was found",
                        vec![dgns.error_ann("subscript without a matching `for`", index.span())],
                        vec![],
                    );
                    Default::default()
                }
            })
        }
    }
}

mod dgns {
    use super::ir::*;
    use crate::dgns::*;

    impl TryHasSpan for DataDefExpr {
        fn try_span(self: &Self) -> Option<Span> {
            self.kind.try_span()
        }
    }

    impl TryHasSpan for DataDefExprKind {
        fn try_span(self: &Self) -> Option<Span> {
            match self {
                DataDefExprKind::Var { var } => Some(var.span()),
                DataDefExprKind::Subscript { array, bracket, .. } => {
                    array.try_span().map(|x| x.join(bracket.span).unwrap())
                }
                DataDefExprKind::Err => None,
            }
        }
    }
}

pub mod mem {
    use crate::mem::*;

    #[derive(Debug)]
    pub enum NodeVal {
        Atom(Box<dyn AtomCell>),
        Array(ArrayVal),
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl EvalMut for DataDefExpr {
        fn eval_mut<'a>(
            self: &Self,
            state: &'a mut State,
            ctx: &mut Context,
        ) -> Result<ExprValMut<'a>, Stop> {
            Ok(match &self.kind {
                DataDefExprKind::Var { var } => {
                    match state.env.get_mut(&var.clone().into()).unwrap() {
                        NodeVal::Atom(atom) => ExprValMut::Atom(&mut **atom),
                        NodeVal::Array(aggr) => ExprValMut::Aggr(aggr),
                    }
                }
                DataDefExprKind::Subscript { array, index, .. } => {
                    let index = index.eval(state, ctx)?;
                    let index = match index {
                        ExprVal::Atom(index) => index,
                        _ => unreachable!(),
                    };
                    let index = index.value_i64() as usize;

                    // FIXME: should evaluate before `index`, but the borrow checker is not happy about it
                    let array = array.eval_mut(state, ctx)?;

                    match array {
                        ExprValMut::Aggr(array) => match array {
                            ArrayVal::AtomArray(array) => ExprValMut::Atom(array.at_mut(index)),
                            ArrayVal::AggrArray(array) => ExprValMut::Aggr(&mut array[index]),
                            ArrayVal::Empty => unreachable!("unallocated array"),
                        },
                        _ => unreachable!(),
                    }
                }
                DataDefExprKind::Err => unreachable!(),
            })
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for DataDefExpr
    where
        DataDefExprKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            self.kind.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for DataDefExprKind
    where
        Name: Gen<L>,
        Expr: Gen<L>,
        DataDefExpr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let ctx = &mut ctx.with_lang(ctx.lang.0);

            match self {
                Self::Var { var } => gen!(ctx, "{}" % (&var.name)),
                Self::Subscript { array, index, .. } => gen!(ctx, "{}[{}]" % (array, index)),
                Self::Err => gen!(ctx, "<<compile-error>>"),
            }
        }
    }

    lang_mixin!(Inspect, DataDefExpr, CommonMixin);
    lang_mixin!(Inspect, DataDefExprKind, CommonMixin);

    impl Gen<Inspect> for DataExprAlloc {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { expr, info } = self;
            gen!(ctx, {
                "<<alloc {} to {}>>" % (expr, info);
            })
        }
    }

    impl Gen<Inspect> for AllocInfo {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { item_ty, size } = self;
            gen!(ctx, "size {} of {}" % (size, item_ty))
        }
    }
}
