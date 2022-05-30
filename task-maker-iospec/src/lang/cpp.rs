use crate::gen::*;
use crate::ir::*;

use crate::lang::clike::*;

pub struct Cpp;

lang_mixin!(Cpp, OuterBlock, CommonMixin);
lang_mixin!(Cpp, InnerBlock, CommonMixin);
lang_mixin!(Cpp, Stmt, CommonMixin);
lang_mixin!(Cpp, StmtKind, CommonMixin);
lang_mixin!(Cpp, MetaStmt, CommonMixin);
lang_mixin!(Cpp, MetaStmtKind, CommonMixin);
lang_mixin!(Cpp, Name, CommonMixin);
lang_mixin!(Cpp, DataDefExpr, CommonMixin);
lang_mixin!(Cpp, DataDefExprKind, CommonMixin);
lang_mixin!(Cpp, Expr, CommonMixin);
lang_mixin!(Cpp, ExprKind, CommonMixin);
lang_mixin!(Cpp, Sign, CommonMixin);
lang_mixin!(Cpp, SumExpr, CommonMixin);
lang_mixin!(Cpp, MulExpr, CommonMixin);
lang_mixin!(Cpp, SubscriptExpr, CommonMixin);
lang_mixin!(Cpp, LitExpr, CommonMixin);
lang_mixin!(Cpp, ParenExpr, CommonMixin);
lang_mixin!(Cpp, RelChainExpr, CommonMixin);
lang_mixin!(Cpp, RelExpr, CommonMixin);
lang_mixin!(Cpp, RelOp, CommonMixin);
lang_mixin!(Cpp, VarExpr, CommonMixin);
lang_mixin!(Cpp, IoStmt, CommonMixin);
lang_mixin!(Cpp, StmtAttr, CommonMixin);
lang_mixin!(Cpp, StmtAttrKind, CommonMixin);
lang_mixin!(Cpp, DocAttr, CommonMixin);
lang_mixin!(Cpp, CfgAttr, CommonMixin);
lang_mixin!(Cpp, ItemStmt, CommonMixin);
lang_mixin!(Cpp, BlockStmt, CommonMixin);
lang_mixin!(Cpp, CallArg, CommonMixin);
lang_mixin!(Cpp, CallArgKind, CommonMixin);
lang_mixin!(Cpp, CallByValueArg, CommonMixin);
lang_mixin!(Cpp, CallRet, CommonMixin);
lang_mixin!(Cpp, CallRetExpr, CommonMixin);
lang_mixin!(Cpp, CallRetKind, CommonMixin);
lang_mixin!(Cpp, SingleCallRet, CommonMixin);
lang_mixin!(Cpp, TupleCallRet, CommonMixin);
lang_mixin!(Cpp, CallMetaStmt, CommonMixin);
lang_mixin!(Cpp, InFunDecl<&Spec>, CommonMixin);

lang_mixin!(Cpp, ForStmt, CLikeMixin);
lang_mixin!(Cpp, IfStmt, CLikeMixin);
lang_mixin!(Cpp, CheckStmt, CLikeMixin);
lang_mixin!(Cpp, SetMetaStmt, CLikeMixin);
lang_mixin!(Cpp, AtomTy, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&CallMetaStmt>, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&CallRet>, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&CallRetExpr>, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&SingleCallRet>, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&CallArg>, CLikeMixin);
lang_mixin!(Cpp, Template<&CallMetaStmt>, CLikeMixin);
lang_mixin!(Cpp, InFunDecl<&Template<&Spec>>, CLikeMixin);

impl Gen<Cpp> for Spec {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let Spec { main, .. } = self;

        gen!(ctx, {
            "#include <vector>";
            "#include <iostream>";
            "#include <cassert>";
            ();
            "using namespace std;";
            (&InFunDecl(self));
            ();
            "int main() {";
            ({ main });
            "}";
        })
    }
}

impl Gen<Cpp> for DataVar {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let Self { name, ty, .. } = self;
        gen!(ctx, {
            "{} {};" % (ty, name);
        })
    }
}

impl Gen<Cpp> for CallByReferenceArg {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let Self { expr, .. } = self;
        gen!(ctx, "{}" % expr)
    }
}

impl Gen<Cpp> for ResizeMetaStmt {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let Self { array, size, .. } = self;
        gen!(ctx, {
            "{}.resize({});" % (array, size);
        })
    }
}

impl Gen<Cpp> for DataExprAlloc {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let Self { expr, info } = self;
        gen!(ctx, {
            "{}.resize({});" % (expr, &info.size);
        })
    }
}

impl Gen<Cpp> for ExprTy {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        match self {
            ExprTy::Atom { atom_ty, .. } => gen!(ctx, "{}" % atom_ty),
            ExprTy::Array { item, .. } => gen!(ctx, "vector<{}>" % item),
            ExprTy::Err => gen!(ctx, "<<unknown-type>>"),
        }
    }
}

impl Gen<Cpp> for InInput<&ItemStmt> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let ItemStmt { expr, .. } = self.0;
        gen!(ctx, {
            "std::cin >> {};" % expr;
        })
    }
}

impl Gen<Cpp> for InOutput<&ItemStmt> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        let ItemStmt { expr, .. } = self.0;
        gen!(ctx, {
            r#"std::cout << {} << " ";"# % expr;
        })
    }
}

impl Gen<Cpp> for InOutput<&Endl> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        gen!(ctx, {
            "std::cout << std::endl;";
        })
    }
}

impl Gen<Cpp> for InInput<&Endl> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        gen!(ctx)
    }
}

impl Gen<Cpp> for InFunDecl<&CallArgKind> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        match &self.0 {
            CallArgKind::Value(arg) => gen!(ctx, "{}" % (&arg.expr.ty)),
            CallArgKind::Reference(arg) => gen!(ctx, "{}&" % (&arg.expr.ty)),
        }
    }
}

impl Gen<Cpp> for Template<&Spec> {
    fn gen(&self, ctx: GenContext<Cpp>) -> Result {
        gen!(ctx, {
            "#include <vector>";
            ();
            "using namespace std;";
            ();
            (&InFunDecl(self));
        })
    }
}
