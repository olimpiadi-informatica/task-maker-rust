pub mod kw {
    syn::custom_keyword!(upto);
}

pub mod ir {
    use crate::ast::kw;
    use crate::ir::*;

    /// IR of the range in a `for` statement.
    #[derive(Debug)]
    pub struct Range {
        pub index: Ir<Name>,
        pub upto: kw::upto,
        pub bound: Ir<RangeBound>,
    }

    /// IR of the range upper bound in a `for` statement.
    #[derive(Debug)]
    pub struct RangeBound {
        pub val: Ir<Expr>,
        pub ty: Ir<AtomTy>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;
    use crate::sem;

    impl CompileFrom<ast::Expr> for RangeBound {
        fn compile(ast: &ast::Expr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let val: Ir<Expr> = ast.compile(env, dgns)?;

            Ok(match val.ty.as_ref() {
                ExprTy::Atom { atom_ty } => {
                    match &atom_ty.sem {
                        None | Some(sem::AtomTy::I32 { .. }) => {}
                        _ => {
                            dgns.error(
                                &format!(
                                    "upper bound of a `for` cycle must be a `i32`, got `{}`",
                                    quote_hir(atom_ty.as_ref()),
                                ),
                                vec![
                                    dgns.error_ann("must be a scalar", val.span()),
                                    dgns.info_ann("type defined here", atom_ty.span()),
                                ],
                                vec![],
                            );
                        }
                    }

                    RangeBound {
                        ty: atom_ty.clone(),
                        val,
                    }
                }
                _ => {
                    dgns.error(
                        &format!(
                            "upper bound of a `for` cycle must be a scalar, got `{}`",
                            quote_hir(val.ty.as_ref()),
                        ),
                        vec![dgns.error_ann("must be a scalar", val.span())],
                        vec![],
                    );

                    RangeBound {
                        ty: Default::default(),
                        val,
                    }
                }
            })
        }
    }
}

mod dgns {
    use crate::dgns::*;
    use crate::ir::*;

    impl HasSpan for Range {
        fn span(self: &Self) -> Span {
            self.index.span().join(self.bound.val.span()).unwrap()
        }
    }
}
