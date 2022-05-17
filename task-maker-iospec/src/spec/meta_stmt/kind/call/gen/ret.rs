use crate::gen::*;
use crate::ir::*;

impl<L> Gen<CommonMixin<'_, L>> for CallRet
where
    CallRetExpr: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        match self.0.as_ref() {
            Some(ret) => gen!(ctx, "{} = " % ret),
            None => gen!(ctx),
        }
    }
}

impl<L> Gen<CommonMixin<'_, L>> for CallRetExpr
where
    CallRetKind: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let Self { kind, .. } = self;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        kind.gen(ctx)
    }
}

impl<L> Gen<CommonMixin<'_, L>> for CallRetKind
where
    SingleCallRet: Gen<L>,
    TupleCallRet: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        match self {
            CallRetKind::Single(ret) => ret.gen(ctx),
            CallRetKind::Tuple(ret) => ret.gen(ctx),
        }
    }
}

impl<L> Gen<CommonMixin<'_, L>> for SingleCallRet
where
    Expr: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let Self { expr } = self;
        expr.gen(&mut ctx.with_lang(ctx.lang.0))
    }
}

impl<L> Gen<CommonMixin<'_, L>> for TupleCallRet
where
    Expr: Gen<L>,
{
    fn gen(&self, _ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        todo!("tuple return value not supported yet")
    }
}
