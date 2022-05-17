use crate::gen::*;
use crate::ir::*;

impl<L> Gen<CommonMixin<'_, L>> for MetaStmt
where
    MetaStmtKind: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let Self { kind, .. } = self;
        kind.gen(&mut ctx.with_lang(ctx.lang.0))
    }
}

impl Gen<Inspect> for MetaStmt {
    fn gen(&self, ctx: GenContext<Inspect>) -> Result {
        let ctx = &mut ctx.with_lang(&CommonMixin(&Inspect));
        gen!(ctx, {
            "@{}" % self;
        })
    }
}

impl<L> Gen<CommonMixin<'_, L>> for MetaStmtKind
where
    SetMetaStmt: Gen<L>,
    CallMetaStmt: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        match self {
            Self::Set(stmt) => stmt.gen(ctx),
            Self::Call(stmt) => stmt.gen(ctx),
        }
    }
}

lang_mixin!(Inspect, MetaStmtKind, CommonMixin);
