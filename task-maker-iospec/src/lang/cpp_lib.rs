use crate::gen::*;
use crate::ir::*;
use crate::sem;

use super::clike::CLikeMixin;
use super::cpp::Cpp;

pub struct CppLib;

lang_mixin!(CppLib, OuterBlock, CommonMixin);
lang_mixin!(CppLib, InnerBlock, CommonMixin);
lang_mixin!(CppLib, Stmt, CommonMixin);
lang_mixin!(CppLib, StmtKind, CommonMixin);
lang_mixin!(CppLib, IoStmt, CommonMixin);
lang_mixin!(CppLib, BlockStmt, CommonMixin);
lang_mixin!(CppLib, MetaStmt, CommonMixin);
lang_mixin!(CppLib, MetaStmtKind, CommonMixin);
lang_mixin!(CppLib, InFunDecl<&Spec>, CommonMixin);

lang_mixin!(CppLib, ForStmt, CLikeMixin);
lang_mixin!(CppLib, IfStmt, CLikeMixin);

lang_same_as!(CppLib, StmtAttr, Cpp);
lang_same_as!(CppLib, DataVar, Cpp);
lang_same_as!(CppLib, DataDefExpr, Cpp);
lang_same_as!(CppLib, Name, Cpp);
lang_same_as!(CppLib, Expr, Cpp);
lang_same_as!(CppLib, ExprTy, Cpp);
lang_same_as!(CppLib, AtomTy, Cpp);
lang_same_as!(CppLib, CallArg, Cpp);
lang_same_as!(CppLib, CallRet, Cpp);
lang_same_as!(CppLib, SetMetaStmt, Cpp);
lang_same_as!(CppLib, InFunDecl<&CallArg>, Cpp);
lang_same_as!(CppLib, InFunDecl<&CallRet>, Cpp);
lang_same_as!(CppLib, InFunDecl<&CallMetaStmt>, Cpp);

impl Gen<CppLib> for Spec {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let Spec { main, .. } = self;

        gen!(ctx, {
            "#ifndef IOLIB_HPP";
            "#define IOLIB_HPP";
            ();
            "#include <vector>";
            "#include <functional>";
            ();
            "using std::vector;";
            ();
            (&InFunDecl(self));
            "struct IoData {";
            ({
                || {
                    for decl in main.decls.iter() {
                        gen!(ctx, {
                            "{} {} = {{}};" % (&decl.ty, &decl.name);
                        })?;
                    }
                    gen!(ctx)
                };

                ();
                "struct Funs {";
                ({
                    || {
                        for call in main.inner.calls.iter() {
                            gen!(ctx, {
                                "std::function<{}({})> {} = [](auto...) {{{}}};"
                                    % (
                                        &InFunDecl(&call.ret),
                                        &Punctuated(
                                            call.args
                                                .iter()
                                                .map(|arg| InFunDecl(arg.as_ref()))
                                                .collect(),
                                            ", ",
                                        ),
                                        &call.name,
                                        &Raw(if call.ret.0.is_some() {
                                            " return 0; "
                                        } else {
                                            ""
                                        }),
                                    );
                            })?;
                        }
                        gen!(ctx)
                    }
                });
                "};";
                ();
                "static Funs global_funs() {";
                ({
                    "Funs funs;";
                    || {
                        for call in main.inner.calls.iter() {
                            gen!(ctx, {
                                "funs.{0} = {0};" % (&call.name);
                            })?;
                        }
                        gen!(ctx)
                    };
                    "return funs;";
                });
                "}";
            });
            "};";
            ();
            "template <";
            "   typename Item,";
            "   typename Endl,";
            "   typename Check,";
            "   typename InvokeVoid,";
            "   typename Invoke,";
            "   typename Resize";
            ">";
            "void process_io(";
            "   IoData& data,";
            "   IoData::Funs funs,";
            "   Item item,";
            "   Endl endl,";
            "   Check check,";
            "   InvokeVoid invoke,";
            "   Invoke invoke_void,";
            "   Resize resize";
            ") {";
            ({
                || {
                    for decl in main.decls.iter() {
                        gen!(ctx, {
                            "auto& {0} = data.{0};" % (&decl.name);
                        })?;
                    }
                    for call in main.inner.calls.iter() {
                        gen!(ctx, {
                            "auto& {0} = funs.{0};" % (&call.name);
                        })?;
                    }
                    ();
                    gen!(ctx, {
                        "const bool INPUT = 0;";
                        "const bool OUTPUT = 1;";
                        ();
                        (&main.inner);
                    })
                }
            });
            "}";
            ();
            "#endif";
        })
    }
}

impl Gen<CppLib> for CheckStmt {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let Self { kw, cond, .. } = self;
        let stream = &kw.to_stream();
        gen!(ctx, {
            "check({}, {});" % (stream, cond);
        })
    }
}

impl Gen<CppLib> for ItemStmt {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let ItemStmt { expr, stream, .. } = self;
        if let Some(stream) = stream {
            gen!(ctx, {
                "item({}, {});" % (stream, expr);
            })
        } else {
            gen!(ctx, {
                "<<item-without-stream>>;";
            })
        }
    }
}

impl Gen<CppLib> for CallMetaStmt {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let Self {
            name, args, ret, ..
        } = self;
        match ret.0.as_ref() {
            Some(ret) => match &ret.kind {
                CallRetKind::Single(ret) => gen!(ctx, {
                    "invoke({}, {}{}{});"
                        % (
                            &ret.expr,
                            name,
                            if args.is_empty() {
                                &Raw("")
                            } else {
                                &Raw(", ")
                            },
                            &Punctuated(args.iter().cloned().collect(), ", "),
                        );
                }),
                CallRetKind::Tuple(_) => todo!(),
            },
            None => gen!(ctx, {
                "invoke_void({}{}{});"
                    % (
                        name,
                        if args.is_empty() {
                            &Raw("")
                        } else {
                            &Raw(", ")
                        },
                        &Punctuated(args.iter().cloned().collect(), ", "),
                    );
            }),
        }
    }
}

impl Gen<CppLib> for sem::Stream {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        match self {
            sem::Stream::Input => gen!(ctx, "INPUT"),
            sem::Stream::Output => gen!(ctx, "OUTPUT"),
        }
    }
}

impl Gen<CppLib> for InOutput<&ItemStmt> {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let ItemStmt { expr, .. } = self.0;
        gen!(ctx, {
            "output_item({});" % expr;
        })
    }
}

impl Gen<CppLib> for InInput<&Endl> {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        gen!(ctx, {
            "endl({});" % (&sem::Stream::Input);
        })
    }
}

impl Gen<CppLib> for InOutput<&Endl> {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        gen!(ctx, {
            "endl({});" % (&sem::Stream::Output);
        })
    }
}

impl Gen<CppLib> for DataExprAlloc {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        let Self { expr, info } = self;
        gen!(ctx, {
            "resize({}, {}, {});"
                % (
                    &expr.root_var.stream.unwrap_or(sem::Stream::Input),
                    expr,
                    &info.size,
                );
        })
    }
}

impl Gen<CppLib> for ResizeMetaStmt {
    fn gen(&self, ctx: GenContext<CppLib>) -> Result {
        // TODO: should we emit something here?
        gen!(ctx)
    }
}
