pub mod ast {
    use syn::token::Bracket;
    use syn::Token;

    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct StmtAttr {
        pub pound: Token![#],
        pub bracket: Bracket,
        pub kind: StmtAttrKind,
    }

    #[derive(Debug, Clone)]
    pub enum StmtAttrKind {
        Doc(DocAttr),
        Cfg(CfgAttr),
        Unknown,
    }

    #[derive(Debug, Clone)]
    pub struct SpecAttr {
        pub pound: Token![#],
        pub bang: Token![!],
        pub bracket: Bracket,
        pub kind: SpecAttrKind,
    }

    #[derive(Debug, Clone)]
    pub enum SpecAttrKind {
        Doc(DocAttr),
        Unknown,
    }
}

mod parse {
    use syn::bracketed;
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for StmtAttr {
        fn parse(input: ParseStream) -> Result<Self> {
            let bracket_input;
            Ok(Self {
                pound: input.parse()?,
                bracket: bracketed!(bracket_input in input),
                kind: bracket_input.parse()?,
            })
        }
    }

    impl Parse for StmtAttrKind {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(kw::doc) {
                Self::Doc(input.parse()?)
            } else if la.peek(kw::cfg) {
                Self::Cfg(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }

    impl Parse for SpecAttr {
        fn parse(input: ParseStream) -> Result<Self> {
            let bracket_input;
            Ok(Self {
                pound: input.parse()?,
                bang: input.parse()?,
                bracket: bracketed!(bracket_input in input),
                kind: bracket_input.parse()?,
            })
        }
    }

    impl Parse for SpecAttrKind {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(kw::doc) {
                Self::Doc(input.parse()?)
            } else {
                Err(la.error())?
            })
        }
    }
}

pub mod ir {
    use crate::ast;

    pub type StmtAttr = ast::StmtAttr;
    pub type StmtAttrKind = ast::StmtAttrKind;

    pub type SpecAttr = ast::SpecAttr;
    pub type SpecAttrKind = ast::SpecAttrKind;
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for StmtAttr
    where
        StmtAttrKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            self.kind.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    impl Gen<Inspect> for StmtAttr {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx, {
                "#[ <<todo>> ]";
            })
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for StmtAttrKind
    where
        CfgAttr: Gen<L>,
        DocAttr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                StmtAttrKind::Cfg(attr) => attr.gen(&mut ctx.with_lang(ctx.lang.0)),
                StmtAttrKind::Doc(attr) => attr.gen(&mut ctx.with_lang(ctx.lang.0)),
                StmtAttrKind::Unknown => gen!(ctx, "<<unknown-attr>>"),
            }
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for SpecAttr
    where
        SpecAttrKind: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            self.kind.gen(&mut ctx.with_lang(ctx.lang.0))
        }
    }

    impl Gen<Inspect> for SpecAttr {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            gen!(ctx, {
                "#![ <<todo>> ]";
            })
        }
    }

    impl<L> Gen<CommonMixin<'_, L>> for SpecAttrKind
    where
        CfgAttr: Gen<L>,
        DocAttr: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match self {
                SpecAttrKind::Doc(attr) => attr.gen(&mut ctx.with_lang(ctx.lang.0)),
                SpecAttrKind::Unknown => gen!(ctx, "<<unknown-attr>>"),
            }
        }
    }

    lang_mixin!(Inspect, SpecAttrKind, CommonMixin);
}
