pub mod kw {
    syn::custom_keyword!(call);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct CallMetaStmt {
        pub kw: kw::call,
        pub name: Name,
        pub paren: syn::token::Paren,
        pub args: syn::punctuated::Punctuated<CallArg, syn::Token![,]>,
        pub ret: Option<CallRet>,
        pub semi: syn::Token![;],
    }

    #[derive(Debug, Clone)]
    pub struct CallArg {
        pub name: Name,
        pub eq: syn::Token![=],
        pub kind: CallArgKind,
    }

    #[derive(Debug, Clone)]
    pub enum CallArgKind {
        Value(CallByValueArg),
        Reference(CallByReferenceArg),
    }

    #[derive(Debug, Clone)]
    pub struct CallByValueArg {
        pub expr: Expr,
    }

    #[derive(Debug, Clone)]
    pub struct CallByReferenceArg {
        pub amp: syn::Token![&],
        pub expr: Expr,
    }

    #[derive(Debug, Clone)]
    pub struct CallRet {
        pub arrow: syn::Token![->],
        pub kind: CallRetKind,
    }

    #[derive(Debug, Clone)]
    pub enum CallRetKind {
        Single(SingleCallRet),
        Tuple(TupleCallRet),
    }

    #[derive(Debug, Clone)]
    pub struct SingleCallRet {
        pub expr: Expr,
    }

    #[derive(Debug, Clone)]
    pub struct TupleCallRet {
        pub paren: syn::token::Paren,
        pub items: syn::punctuated::Punctuated<Expr, syn::Token![,]>,
    }
}

mod parse {
    use syn::parenthesized;
    use syn::parse::*;
    use syn::punctuated::Punctuated;
    use syn::Token;

    use crate::ast::*;

    impl Parse for CallMetaStmt {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;
            Ok(Self {
                kw: input.parse()?,
                name: input.parse()?,
                paren: parenthesized!(paren_input in input),
                args: Punctuated::parse_terminated(&paren_input)?,
                ret: if input.peek(Token![->]) {
                    Some(input.parse()?)
                } else {
                    None
                },
                semi: input.parse()?,
            })
        }
    }

    impl Parse for CallArg {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                name: input.parse()?,
                eq: input.parse()?,
                kind: input.parse()?,
            })
        }
    }

    impl Parse for CallArgKind {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(if input.peek(Token![&]) {
                Self::Reference(input.parse()?)
            } else {
                Self::Value(input.parse()?)
            })
        }
    }

    impl Parse for CallByValueArg {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                expr: input.parse()?,
            })
        }
    }

    impl Parse for CallByReferenceArg {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                amp: input.parse()?,
                expr: input.parse()?,
            })
        }
    }

    impl Parse for CallRet {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                arrow: input.parse()?,
                kind: input.parse()?,
            })
        }
    }

    impl Parse for CallRetKind {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(if input.peek(syn::token::Paren) {
                Self::Tuple(input.parse()?)
            } else {
                Self::Single(input.parse()?)
            })
        }
    }

    impl Parse for SingleCallRet {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                expr: input.parse()?,
            })
        }
    }

    impl Parse for TupleCallRet {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;
            Ok(Self {
                paren: parenthesized!(paren_input in input),
                items: Punctuated::parse_separated_nonempty(&paren_input)?,
            })
        }
    }
}

pub mod ir {
    use crate::ast;
    use crate::ir::*;

    #[derive(Debug)]
    pub struct CallMetaStmt {
        pub kw: ast::kw::call,
        pub name: Name,
        pub paren: syn::token::Paren,
        pub arg_commas: Vec<syn::Token![,]>,
        pub args: Vec<Ir<CallArg>>,
        pub ret: CallRet,
        pub semi: syn::Token![;],
    }

    #[derive(Debug)]
    pub struct CallRet(pub Option<CallRetExpr>);

    #[derive(Debug)]
    pub struct CallArg {
        pub name: Ir<Name>,
        pub eq: syn::Token![=],
        pub kind: CallArgKind,
    }

    #[derive(Debug)]
    pub enum CallArgKind {
        Value(CallByValueArg),
        Reference(CallByReferenceArg),
    }

    #[derive(Debug)]
    pub struct CallByValueArg {
        pub expr: Expr,
    }

    #[derive(Debug)]
    pub struct CallByReferenceArg {
        pub amp: syn::Token![&],
        pub expr: Expr,
    }

    #[derive(Debug)]
    pub struct CallRetExpr {
        pub arrow: syn::Token![->],
        pub kind: CallRetKind,
    }

    #[derive(Debug)]
    pub enum CallRetKind {
        Single(SingleCallRet),
        Tuple(TupleCallRet),
    }

    #[derive(Debug)]
    pub struct SingleCallRet {
        pub expr: Expr,
    }

    #[derive(Debug)]
    pub struct TupleCallRet {
        pub paren: syn::token::Paren,
        pub items: Vec<Expr>,
        pub item_commas: Vec<syn::Token![,]>,
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::CallMetaStmt> for CallMetaStmt {
        fn compile(
            ast: &ast::CallMetaStmt,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::CallMetaStmt {
                kw,
                name,
                paren,
                args,
                ret,
                semi,
            } = ast;

            let (args, arg_commas) = unzip_punctuated(args.clone());

            Ok(Self {
                kw: kw.clone(),
                name: name.compile(env, dgns)?,
                paren: paren.clone(),
                args: args
                    .iter()
                    .map(|a| a.compile(env, dgns))
                    .collect::<Result<_>>()?,
                arg_commas,
                ret: CallRet(ret.as_ref().map(|ret| ret.compile(env, dgns)).transpose()?),
                semi: semi.clone(),
            })
        }
    }

    impl CompileFrom<ast::CallArg> for CallArg {
        fn compile(ast: &ast::CallArg, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CallArg { name, eq, kind } = ast;

            Ok(Self {
                name: name.compile(env, dgns)?,
                eq: eq.clone(),
                kind: kind.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::CallArgKind> for CallArgKind {
        fn compile(
            ast: &ast::CallArgKind,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match ast {
                ast::CallArgKind::Value(arg) => CallArgKind::Value(arg.compile(env, dgns)?),
                ast::CallArgKind::Reference(arg) => CallArgKind::Reference(arg.compile(env, dgns)?),
            })
        }
    }

    impl CompileFrom<ast::CallByValueArg> for CallByValueArg {
        fn compile(
            ast: &ast::CallByValueArg,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::CallByValueArg { expr } = ast;
            Ok(Self {
                expr: expr.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::CallByReferenceArg> for CallByReferenceArg {
        fn compile(
            ast: &ast::CallByReferenceArg,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::CallByReferenceArg { amp, expr } = ast;

            Ok(Self {
                amp: amp.clone(),
                expr: expr.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::CallRet> for CallRetExpr {
        fn compile(ast: &ast::CallRet, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CallRet { arrow, kind } = ast;

            Ok(Self {
                arrow: arrow.clone(),
                kind: kind.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::CallRetKind> for CallRetKind {
        fn compile(
            ast: &ast::CallRetKind,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match ast {
                ast::CallRetKind::Single(ret) => Self::Single(ret.compile(env, dgns)?),
                ast::CallRetKind::Tuple(ret) => Self::Tuple(ret.compile(env, dgns)?),
            })
        }
    }

    impl CompileFrom<ast::SingleCallRet> for SingleCallRet {
        fn compile(
            ast: &ast::SingleCallRet,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::SingleCallRet { expr } = ast;

            Ok(Self {
                expr: expr.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::TupleCallRet> for TupleCallRet {
        fn compile(
            ast: &ast::TupleCallRet,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            let ast::TupleCallRet { items, paren } = ast;

            let (items, item_commas) = unzip_punctuated(items.clone());

            Ok(Self {
                paren: paren.clone(),
                items: items
                    .iter()
                    .map(|item| item.compile(env, dgns))
                    .collect::<Result<_>>()?,
                item_commas,
            })
        }
    }
}

pub mod gen {
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
}
