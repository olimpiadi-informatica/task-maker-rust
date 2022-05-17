use crate::gen::*;
use crate::ir::*;

impl<L> Gen<CommonMixin<'_, L>> for CallArg
where
    CallArgKind: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        self.kind.gen(&mut ctx.with_lang(ctx.lang.0))
    }
}

impl<L> Gen<CommonMixin<'_, L>> for CallArgKind
where
    CallByValueArg: Gen<L>,
    CallByReferenceArg: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        match self {
            CallArgKind::Value(arg) => arg.gen(&mut ctx.with_lang(ctx.lang.0)),
            CallArgKind::Reference(arg) => arg.gen(&mut ctx.with_lang(ctx.lang.0)),
        }
    }
}

impl<L> Gen<CommonMixin<'_, L>> for CallByValueArg
where
    Expr: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        self.expr.gen(&mut ctx.with_lang(ctx.lang.0))
    }
}
