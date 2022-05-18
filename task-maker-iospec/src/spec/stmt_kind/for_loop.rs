pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct ForStmt {
        pub kw: syn::Token![for],
        pub index: Name,
        pub upto: kw::upto,
        pub bound: Expr,
        pub body: BracedBlock,
    }
}

mod parse {
    use crate::ast;
    use syn::parse::*;

    impl Parse for ast::ForStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                index: input.parse()?,
                upto: input.parse()?,
                bound: input.parse()?,
                body: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ir::*;

    #[derive(Debug)]
    pub struct ForStmt<T = Ir<MetaStmtKind>> {
        pub kw: syn::token::For,
        pub range: Ir<Range>,
        pub body: Ir<OuterBlock<T>>,
        pub data_defs: Vec<Ir<DataDefExpr>>,
        pub allocs: Vec<DataExprAlloc>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::ForStmt> for ForStmt<ast::MetaStmtKind> {
        fn compile(ast: &ast::ForStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::ForStmt {
                kw,
                index,
                upto,
                bound,
                body,
            } = ast;

            let range = Ir::new(Range {
                index: index.compile(env, dgns)?,
                upto: upto.clone(),
                bound: bound.compile(env, dgns)?,
            });

            let block: InnerBlock<ast::MetaStmtKind> =
                (&body.content).compile(&env.for_body(range.clone()), dgns)?;

            let inner_decl_exprs: Vec<Ir<DataDefExpr>> = block
                .data_defs
                .iter()
                .filter_map(|expr| match &expr.kind {
                    DataDefExprKind::Var { .. } => Some(expr.clone()),
                    _ => None,
                })
                .collect();

            let outer_decl_exprs: Vec<Ir<DataDefExpr>> = block
                .data_defs
                .iter()
                .filter_map(|node| match &node.kind {
                    DataDefExprKind::Subscript { array, .. } => Some(array.clone()),
                    _ => None,
                })
                .collect();

            let allocs: Vec<_> = outer_decl_exprs
                .iter()
                .flat_map(|expr| {
                    expr.alloc.as_ref().map(|alloc| DataExprAlloc {
                        info: alloc.clone(),
                        expr: expr.clone(),
                    })
                })
                .collect();

            Ok(Self {
                kw: kw.clone(),
                range,
                body: OuterBlock::new(inner_decl_exprs, block),
                data_defs: outer_decl_exprs,
                allocs,
            })
        }
    }

    impl CompileFrom<ForStmt<ast::MetaStmtKind>> for ForStmt {
        fn compile(
            input: &ForStmt<ast::MetaStmtKind>,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ForStmt {
                kw,
                range,
                body,
                data_defs,
                allocs,
            } = input;
            Ok(Self {
                kw: kw.clone(),
                range: range.clone(),
                body: body.as_ref().compile(&env.for_body(range.clone()), dgns)?,
                data_defs: data_defs.clone(),
                allocs: allocs.clone(),
            })
        }
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;

    impl Run for ForStmt {
        fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop> {
            let bound = self.range.bound.val.eval(state, ctx)?;
            let bound = match bound {
                ExprVal::Atom(bound) => bound.value_i64() as usize,
                ExprVal::Array(_) => unreachable!(),
            };

            for node in self.data_defs.iter() {
                if let Some(var) = &node.var {
                    state.decl(&var);
                }

                if let Some(_) = &node.alloc {
                    node.eval_mut(state, ctx)?.alloc(node, bound);
                }
            }

            for i in 0..bound {
                state.indexes.insert(self.range.clone().into(), i);
                self.body.run(state, ctx)?;
                state.indexes.remove(&self.range.clone().into());
            }

            Ok(())
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for ForStmt {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { range, body, .. } = self;

            gen!(ctx, {
                "for {}:" % range;
                ({ body });
            })
        }
    }

    impl Gen<Inspect> for Range {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { index, bound, .. } = self;
            gen!(ctx, "{} upto {}" % (index, bound))
        }
    }

    impl Gen<Inspect> for RangeBound {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            let Self { val, ty, .. } = self;
            gen!(ctx, "{} <{}>" % (val, ty))
        }
    }
}
