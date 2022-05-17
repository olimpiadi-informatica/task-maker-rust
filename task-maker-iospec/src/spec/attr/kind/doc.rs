pub mod kw {
    syn::custom_keyword!(doc);
}

pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct DocAttr {
        pub kw: kw::doc,
        pub eq: syn::Token![=],
        pub str: syn::LitStr,
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for DocAttr {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kw: input.parse()?,
                eq: input.parse()?,
                str: input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ast;

    pub type DocAttr = ast::DocAttr;
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for DocAttr
    where
        DataDefExpr: Gen<L>,
        AtomTy: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            let Self { str, .. } = self;
            let str = str.value();
            gen!(ctx, {
                "/**{} */" % (&Raw(str));
            })
        }
    }

    lang_mixin!(Inspect, DocAttr, CommonMixin);
}
