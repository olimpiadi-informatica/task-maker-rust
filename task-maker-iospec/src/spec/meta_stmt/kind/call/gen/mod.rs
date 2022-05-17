use crate::gen::*;
use crate::ir::*;

mod arg;
pub use arg::*;

mod ret;
pub use ret::*;

pub struct InFunDecl<T>(pub T);

impl<L> Gen<CommonMixin<'_, L>> for CallMetaStmt
where
    CallRet: Gen<L>,
    Name: Gen<L>,
    CallArg: Gen<L>,
{
    fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
        let Self {
            name, args, ret, ..
        } = self;
        let ctx = &mut ctx.with_lang(ctx.lang.0);
        gen!(ctx, {
            "{}{}({});" % (ret, name, &Punctuated(args.iter().cloned().collect(), ", "));
        })
    }
}

impl Gen<Inspect> for CallMetaStmt {
    fn gen(&self, ctx: GenContext<Inspect>) -> Result {
        gen!(ctx, "call (<<todo>>)")
    }
}
