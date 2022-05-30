pub mod kw {
    syn::custom_keyword!(resize);
    syn::custom_keyword!(to);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct ResizeMetaStmt {
        pub kw: kw::resize,
        pub array: Expr,
        pub to_kw: kw::to,
        pub size: Expr,
        pub semi: syn::Token![;],
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for ResizeMetaStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                array: input.parse()?,
                to_kw: input.parse()?,
                size: input.parse()?,
                semi: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ast;
    use crate::ir::*;

    #[derive(Debug)]
    pub struct ResizeMetaStmt {
        pub kw: ast::kw::resize,
        pub array: Ir<Expr>,
        pub item_ty: Option<Ir<ExprTy>>,
        pub to_kw: ast::kw::to,
        pub size: Ir<Expr>,
        pub semi: syn::Token![;],
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::ResizeMetaStmt> for ResizeMetaStmt {
        fn compile(
            ast: &ast::ResizeMetaStmt,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::ResizeMetaStmt {
                kw,
                array,
                to_kw,
                size,
                semi,
            } = ast;

            let array: Ir<Expr> = array.compile(env, dgns)?;

            let item_ty = match array.ty.as_ref() {
                ExprTy::Array { item, .. } => Some(item.clone()),
                _ => {
                    dgns.error(
                        &format!("expected array type, got {}", quote_hir(&array.ty)),
                        vec![dgns.error_ann("expected array type", array.span())],
                        vec![],
                    );
                    None
                }
            };

            Ok(Self {
                kw: kw.clone(),
                array,
                item_ty,
                to_kw: to_kw.clone(),
                size: size.compile(env, dgns)?,
                semi: semi.clone(),
            })
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for ResizeMetaStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx, "call (<<todo>>)")
        }
    }
}
