pub mod ir {
    use crate::ir::*;
    use crate::sem;

    /// IR a variable containing input/output data.
    /// E.g., `A` in `... read A[i][j]: n32; ...`.
    #[derive(Debug)]
    pub struct DataVar {
        pub name: Ir<Name>,
        pub ty: Ir<ExprTy>,
        pub stream: Option<sem::Stream>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::Name, DataDefEnv> for DataVar {
        fn compile(
            ast: &ast::Name,
            env: &DataDefEnv,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(Self {
                name: ast.compile(env.outer.as_ref(), dgns)?,
                ty: env.ty.clone(),
                stream: env.outer.cur_io.as_ref().map(|kw| kw.to_stream()),
            })
        }
    }
}

mod dgns {
    use super::ir::*;
    use crate::dgns::*;

    impl HasSpan for DataVar {
        fn span(self: &Self) -> Span {
            self.name.span()
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for DataVar {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { name, ty, .. } = self;
            gen!(ctx, {
                "<<decl {} of type {}>>" % (name, ty);
            })
        }
    }
}
