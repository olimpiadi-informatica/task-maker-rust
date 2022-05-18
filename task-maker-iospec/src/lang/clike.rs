use crate::gen::*;
use crate::ir::*;
use crate::sem;

pub struct CLikeMixin<'a, L>(pub &'a L);

impl<L> Gen<CLikeMixin<'_, L>> for ForStmt
where
    AtomTy: Gen<L>,
    Name: Gen<L>,
    Expr: Gen<L>,
    OuterBlock: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        let Self { range, body, .. } = self;
        let Range { index, bound, .. } = range.as_ref();
        let RangeBound { val, ty, .. } = bound.as_ref();
        gen!(ctx, {
            "for({0} {1} = 0; {1} < {2}; {1}++) {{" % (ty, index, val);
            ({ body });
            "}";
        })
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for IfStmt
where
    Expr: Gen<L>,
    InnerBlock: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        let Self { cond, body, .. } = self;
        gen!(ctx, {
            "if({}) {{" % cond;
            ({ body });
            "}";
        })
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for SetMetaStmt
where
    Expr: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let Self { lexpr, rexpr, .. } = self;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        gen!(ctx, {
            "{} = {};" % (lexpr, rexpr);
        })
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for AtomTy {
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        match self.sem {
            Some(ty) => match ty {
                sem::AtomTy::Bool => gen!(ctx, "bool"),
                sem::AtomTy::I32 => gen!(ctx, "int"),
                sem::AtomTy::I64 => gen!(ctx, "long long"),
            },
            _ => gen!(ctx, "<<compile-error>>"),
        }
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&CallMetaStmt>
where
    for<'a> InFunDecl<&'a CallRet>: Gen<L>,
    for<'a> InFunDecl<&'a CallArg>: Gen<L>,
    Name: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let CallMetaStmt {
            ret, name, args, ..
        } = self.0;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        gen!(ctx, {
            "{} {}({});"
                % (
                    &InFunDecl(ret),
                    name,
                    &Punctuated(
                        args.iter().map(|arg| InFunDecl(arg.as_ref())).collect(),
                        ", ",
                    ),
                );
        })
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&CallRet>
where
    for<'a> InFunDecl<&'a CallRetExpr>: Gen<L>,
    Name: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        match self.0 .0.as_ref() {
            Some(ret) => InFunDecl(ret).gen(&mut ctx.with_lang(ctx.lang.0)),
            None => gen!(ctx, "void"),
        }
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&CallRetExpr>
where
    for<'a> InFunDecl<&'a SingleCallRet>: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        match &self.0.kind {
            CallRetKind::Single(ret) => InFunDecl(ret).gen(ctx),
            CallRetKind::Tuple(_) => todo!(),
        }
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&SingleCallRet>
where
    ExprTy: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let SingleCallRet { expr, .. } = self.0;
        expr.ty.gen(&mut ctx.with_lang(ctx.lang.0))
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&CallArg>
where
    for<'a> InFunDecl<&'a CallArgKind>: Gen<L>,
    Name: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let CallArg { name, kind, .. } = self.0;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        gen!(ctx, "{} {}" % (&InFunDecl(kind), name))
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for CheckStmt
where
    Expr: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let Self { cond, .. } = self;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        gen!(ctx, {
            "assert({});" % cond;
        })
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for InFunDecl<&Template<&Spec>>
where
    for<'a> Template<&'a CallMetaStmt>: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let Spec { main, .. } = self.0 .0;
        let calls = &main.inner.calls;
        let ctx = &mut ctx.with_lang(ctx.lang.0);

        let mut needs_empty_line = false;
        for call in calls {
            if needs_empty_line {
                gen!(ctx, {
                    ();
                })?;
            }
            ctx.gen(&Template(call.as_ref()))?;
            needs_empty_line = true;
        }
        gen!(ctx)
    }
}

impl<L> Gen<CLikeMixin<'_, L>> for Template<&CallMetaStmt>
where
    for<'a> InFunDecl<&'a CallRet>: Gen<L>,
    for<'a> InFunDecl<&'a CallArg>: Gen<L>,
    Name: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CLikeMixin<L>>) -> Result {
        let CallMetaStmt {
            ret, name, args, ..
        } = self.0;
        let ctx = &mut ctx.with_lang(ctx.lang.0);

        gen!(ctx, {
            "{} {}({}) {{"
                % (
                    &InFunDecl(ret),
                    name,
                    &Punctuated(
                        args.iter().map(|arg| InFunDecl(arg.as_ref())).collect(),
                        ", ",
                    ),
                );
            ({
                || {
                    match ret.0.as_ref() {
                        Some(ret) => match &ret.kind {
                            CallRetKind::Single(ret) => match ret.expr.ty.as_ref() {
                                ExprTy::Atom { .. } => gen!(ctx, {
                                    "return 42;";
                                })?,
                                _ => (),
                            },
                            _ => (),
                        },
                        None => (),
                    };
                    gen!(ctx)
                };
            });
            "}";
        })
    }
}
