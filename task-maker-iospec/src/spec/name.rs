pub mod ast {
    /// AST of an identifier

    #[derive(Debug, Clone)]
    pub struct Name {
        pub ident: proc_macro2::Ident,
    }
}

mod parse {
    use crate::ast::*;

    use syn::parse::*;

    impl Parse for Name {
        fn parse(input: ParseStream) -> Result<Self> {
            // Parsing TokenTree instead of Indent to ignore Rust keywords
            let token_tree: proc_macro2::TokenTree = input.parse()?;
            match token_tree {
                proc_macro2::TokenTree::Ident(ident) => Ok(Self { ident }),
                _ => Err(Error::new(token_tree.span(), "expected identifier")),
            }
        }
    }
}

pub mod ir {
    /// IR of an identifier
    pub type Name = crate::ast::Name;
}

mod compile {
    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;

    impl CompileFrom<ast::Name> for Name {
        fn compile(ast: &ast::Name, _env: &Env, _dgns: &mut DiagnosticContext) -> Result<Self> {
            Ok(ast.clone())
        }
    }
}

mod dgns {
    use crate::dgns::*;
    use crate::ir::*;

    impl HasSpan for Name {
        fn span(self: &Self) -> Span {
            self.ident.span()
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl<L> Gen<CommonMixin<'_, L>> for Name {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            ctx.append(&self.ident)
        }
    }

    lang_mixin!(Inspect, Name, CommonMixin);
}
