pub mod kw {
    syn::custom_keyword!(cfg);
}
pub mod ast {
    use crate::ast::*;

    #[derive(Debug, Clone)]
    pub struct CfgAttr {
        pub kw: kw::cfg,
        pub paren: syn::token::Paren,
        pub expr: CfgExpr,
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for CfgAttr {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;

            Ok(Self {
                kw: input.parse()?,
                paren: syn::parenthesized!(paren_input in input),
                expr: paren_input.parse()?,
            })
        }
    }
}

pub mod ir {
    use crate::ast;

    pub type CfgAttr = ast::CfgAttr;
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for CfgAttr
    where
        DataDefExpr: Gen<L>,
        AtomTy: Gen<L>,
    {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            gen!(ctx)
        }
    }

    lang_mixin!(Inspect, CfgAttr, CommonMixin);
}
