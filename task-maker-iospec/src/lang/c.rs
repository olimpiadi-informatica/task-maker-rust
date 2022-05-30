use crate::gen::*;
use crate::ir::*;
use crate::sem;

use crate::lang::clike::*;

pub struct C;

lang_mixin!(C, OuterBlock, CommonMixin);
lang_mixin!(C, InnerBlock, CommonMixin);
lang_mixin!(C, Stmt, CommonMixin);
lang_mixin!(C, StmtKind, CommonMixin);
lang_mixin!(C, MetaStmt, CommonMixin);
lang_mixin!(C, MetaStmtKind, CommonMixin);
lang_mixin!(C, Name, CommonMixin);
lang_mixin!(C, DataDefExpr, CommonMixin);
lang_mixin!(C, DataDefExprKind, CommonMixin);
lang_mixin!(C, Expr, CommonMixin);
lang_mixin!(C, ExprKind, CommonMixin);
lang_mixin!(C, Sign, CommonMixin);
lang_mixin!(C, SumExpr, CommonMixin);
lang_mixin!(C, MulExpr, CommonMixin);
lang_mixin!(C, SubscriptExpr, CommonMixin);
lang_mixin!(C, LitExpr, CommonMixin);
lang_mixin!(C, ParenExpr, CommonMixin);
lang_mixin!(C, RelChainExpr, CommonMixin);
lang_mixin!(C, RelExpr, CommonMixin);
lang_mixin!(C, RelOp, CommonMixin);
lang_mixin!(C, VarExpr, CommonMixin);
lang_mixin!(C, IoStmt, CommonMixin);
lang_mixin!(C, StmtAttr, CommonMixin);
lang_mixin!(C, StmtAttrKind, CommonMixin);
lang_mixin!(C, CfgAttr, CommonMixin);
lang_mixin!(C, DocAttr, CommonMixin);
lang_mixin!(C, ItemStmt, CommonMixin);
lang_mixin!(C, CallArg, CommonMixin);
lang_mixin!(C, CallArgKind, CommonMixin);
lang_mixin!(C, CallByValueArg, CommonMixin);
lang_mixin!(C, CallRet, CommonMixin);
lang_mixin!(C, CallRetExpr, CommonMixin);
lang_mixin!(C, CallRetKind, CommonMixin);
lang_mixin!(C, SingleCallRet, CommonMixin);
lang_mixin!(C, TupleCallRet, CommonMixin);
lang_mixin!(C, CallMetaStmt, CommonMixin);
lang_mixin!(C, InFunDecl<&Spec>, CommonMixin);
lang_mixin!(C, BlockStmt, CommonMixin);

lang_mixin!(C, ForStmt, CLikeMixin);
lang_mixin!(C, IfStmt, CLikeMixin);
lang_mixin!(C, CheckStmt, CLikeMixin);
lang_mixin!(C, SetMetaStmt, CLikeMixin);
lang_mixin!(C, AtomTy, CLikeMixin);
lang_mixin!(C, InFunDecl<&CallMetaStmt>, CLikeMixin);
lang_mixin!(C, InFunDecl<&CallRet>, CLikeMixin);
lang_mixin!(C, InFunDecl<&CallRetExpr>, CLikeMixin);
lang_mixin!(C, InFunDecl<&SingleCallRet>, CLikeMixin);
lang_mixin!(C, InFunDecl<&CallArg>, CLikeMixin);
lang_mixin!(C, Template<&CallMetaStmt>, CLikeMixin);
lang_mixin!(C, InFunDecl<&Template<&Spec>>, CLikeMixin);

impl Gen<C> for Spec {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Spec { main, .. } = self;

        gen!(ctx, {
            "#include <stdio.h>";
            "#include <stdlib.h>";
            "#include <stdbool.h>";
            "#include <assert.h>";
            (&InFunDecl(self));
            ();
            "int main() {";
            ({ main });
            "}";
        })
    }
}

impl Gen<C> for CallByReferenceArg {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Self { expr, .. } = self;
        match expr.ty.as_ref() {
            // Arrays are pointers already
            ExprTy::Array { .. } => gen!(ctx, "{}" % expr),
            _ => gen!(ctx, "&{}" % (expr)),
        }
    }
}

impl Gen<C> for DataExprAlloc {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Self { expr, info } = self;
        gen!(ctx, {
            "{0} = realloc({0}, {1});" % (expr, info);
        })
    }
}

impl Gen<C> for ResizeMetaStmt {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Self {
            array,
            item_ty,
            size,
            ..
        } = self;
        match item_ty.as_ref() {
            Some(item_ty) => gen!(ctx, {
                "{0} = realloc({0}, {1});"
                    % (
                        array,
                        &AllocInfo {
                            item_ty: item_ty.clone(),
                            size: size.clone(),
                        },
                    );
            }),
            None => gen!(ctx),
        }
    }
}

impl Gen<C> for AllocInfo {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Self { item_ty, size } = self;
        gen!(ctx, "sizeof({}) * ({})" % (item_ty, size))
    }
}

impl Gen<C> for ExprTy {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        match self {
            ExprTy::Atom { atom_ty, .. } => gen!(ctx, "{}" % atom_ty),
            ExprTy::Array { item, .. } => gen!(ctx, "{}*" % item),
            ExprTy::Err => gen!(ctx, "<<unknown-type>>"),
        }
    }
}

struct Format<T>(pub T);

impl Gen<C> for InInput<&ItemStmt> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let ItemStmt { expr, .. } = self.0;
        gen!(ctx, {
            r#"assert(scanf("{}", &{}) == 1);"# % (&InInput(&Format(self.0)), expr);
        })
    }
}

impl Gen<C> for InInput<&Format<&ItemStmt>> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        match self.0 .0.ty.sem {
            Some(sem::AtomTy::Bool | sem::AtomTy::I32) => gen!(ctx, "%d"),
            Some(sem::AtomTy::I64) => gen!(ctx, "%lld"),
            _ => gen!(ctx, "<<unsupported-scalar>>"),
        }
    }
}

impl Gen<C> for InOutput<&ItemStmt> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let ItemStmt { expr, .. } = self.0;
        gen!(ctx, {
            r#"printf("{}", {});"# % (&InOutput(&Format(self.0)), expr);
        })
    }
}

impl Gen<C> for InOutput<&Format<&ItemStmt>> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        match self.0 .0.ty.sem {
            Some(sem::AtomTy::Bool | sem::AtomTy::I32) => gen!(ctx, "%d "),
            Some(sem::AtomTy::I64) => gen!(ctx, "%lld "),
            _ => gen!(ctx, "<<unsupported-scalar>>"),
        }
    }
}

impl Gen<C> for InOutput<&Endl> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        gen!(ctx, {
            r#"printf("\n");"#;
        })
    }
}

impl Gen<C> for InInput<&Endl> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        gen!(ctx)
    }
}

impl Gen<C> for InFunDecl<&CallArgKind> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        match &self.0 {
            CallArgKind::Value(arg) => gen!(ctx, "{}" % (&arg.expr.ty)),
            CallArgKind::Reference(arg) => match arg.expr.ty.as_ref() {
                // Arrays are pointers already
                ExprTy::Array { .. } => gen!(ctx, "{}" % (&arg.expr.ty)),
                _ => gen!(ctx, "{}*" % (&arg.expr.ty)),
            },
        }
    }
}

impl Gen<C> for Template<&Spec> {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        gen!(ctx, {
            "#include <stdbool.h>";
            ();
            (&InFunDecl(self));
        })
    }
}

impl Gen<C> for DataVar {
    fn gen(&self, ctx: GenContext<C>) -> Result {
        let Self { name, ty, .. } = self;
        gen!(ctx, {
            "{} {} = 0;" % (ty, name);
        })
    }
}
