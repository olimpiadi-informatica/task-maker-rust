pub mod kw {
    syn::custom_keyword!(not);
    syn::custom_keyword!(any);
    syn::custom_keyword!(all);
}

pub mod ast {
    use crate::ast::*;

    /// AST of, e.g., `not(lang = "cpp")` in `#[cfg(not(lang = "cpp"))]`.
    #[derive(Debug, Clone)]
    pub struct CfgExpr {
        pub kind: CfgExprKind,
    }

    #[derive(Debug, Clone)]
    pub enum CfgExprKind {
        IsDef(CfgIsOnExpr),
        Is(CfgIsExpr),
        Not(CfgNotExpr),
        Any(CfgAnyExpr),
        All(CfgAllExpr),
    }

    /// AST of, e.g., `grader` in `#[cfg(grader)]`.
    #[derive(Debug, Clone)]
    pub struct CfgIsOnExpr {
        pub name: Name,
    }

    /// AST of, e.g., `lang = "C"` in `#[cfg(lang = "C")]`.
    #[derive(Debug, Clone)]
    pub struct CfgIsExpr {
        pub name: Name,
        pub eq: syn::Token![=],
        pub val: syn::LitStr,
    }

    /// AST of, e.g., `not(...)` in `#[cfg(not(...))]`.
    #[derive(Debug, Clone)]
    pub struct CfgNotExpr {
        pub kw: kw::not,
        pub paren: syn::token::Paren,
        pub arg: Box<CfgExpr>,
    }

    /// AST of, e.g., `any(...)` in `#[cfg(any(...))]`.
    #[derive(Debug, Clone)]
    pub struct CfgAnyExpr {
        pub kw: kw::any,
        pub paren: syn::token::Paren,
        pub args: syn::punctuated::Punctuated<CfgExpr, syn::Token![,]>,
    }

    /// AST of, e.g., `all(...)` in `#[cfg(all(...))]`.
    #[derive(Debug, Clone)]
    pub struct CfgAllExpr {
        pub kw: kw::all,
        pub paren: syn::token::Paren,
        pub args: syn::punctuated::Punctuated<CfgExpr, syn::Token![,]>,
    }
}

mod parse {
    use syn::parse::*;

    use crate::ast::*;

    impl Parse for CfgExpr {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                kind: input.parse()?,
            })
        }
    }

    impl Parse for CfgExprKind {
        fn parse(input: ParseStream) -> Result<Self> {
            let la = input.lookahead1();
            Ok(if la.peek(kw::not) {
                Self::Not(input.parse()?)
            } else if la.peek(kw::any) {
                Self::Any(input.parse()?)
            } else if la.peek(kw::all) {
                Self::All(input.parse()?)
            } else {
                let name: Name = input.parse()?;
                if input.peek(syn::Token![=]) {
                    Self::Is(CfgIsExpr {
                        name,
                        eq: input.parse()?,
                        val: input.parse()?,
                    })
                } else {
                    Self::IsDef(CfgIsOnExpr { name })
                }
            })
        }
    }

    impl Parse for CfgNotExpr {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;
            Ok(Self {
                kw: input.parse()?,
                paren: syn::parenthesized!(paren_input in input),
                arg: paren_input.parse()?,
            })
        }
    }

    impl Parse for CfgAnyExpr {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;
            Ok(Self {
                kw: input.parse()?,
                paren: syn::parenthesized!(paren_input in input),
                args: syn::punctuated::Punctuated::parse_terminated(&paren_input)?,
            })
        }
    }

    impl Parse for CfgAllExpr {
        fn parse(input: ParseStream) -> Result<Self> {
            let paren_input;
            Ok(Self {
                kw: input.parse()?,
                paren: syn::parenthesized!(paren_input in input),
                args: syn::punctuated::Punctuated::parse_terminated(&paren_input)?,
            })
        }
    }
}

mod compile {
    use crate::ast;
    use crate::compile::*;

    impl CompileFrom<ast::CfgExpr> for bool {
        fn compile(ast: &ast::CfgExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CfgExpr { kind, .. } = ast;
            Ok(kind.compile(env, dgns)?)
        }
    }

    impl CompileFrom<ast::CfgExprKind> for bool {
        fn compile(
            ast: &ast::CfgExprKind,
            env: &Env,
            dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(match ast {
                ast::CfgExprKind::IsDef(expr) => expr.compile(env, dgns)?,
                ast::CfgExprKind::Is(expr) => expr.compile(env, dgns)?,
                ast::CfgExprKind::Not(expr) => expr.compile(env, dgns)?,
                ast::CfgExprKind::Any(expr) => expr.compile(env, dgns)?,
                ast::CfgExprKind::All(expr) => expr.compile(env, dgns)?,
            })
        }
    }

    impl CompileFrom<ast::CfgIsExpr> for bool {
        fn compile(ast: &ast::CfgIsExpr, env: &Env, _dgns: &mut DiagnosticContext) -> Result<Self> {
            Ok(env.cfg.is(&ast.name.ident.to_string(), &ast.val.value()))
        }
    }

    impl CompileFrom<ast::CfgIsOnExpr> for bool {
        fn compile(
            ast: &ast::CfgIsOnExpr,
            env: &Env,
            _dgns: &mut DiagnosticContext,
        ) -> Result<Self> {
            Ok(env.cfg.is_on(&ast.name.ident.to_string()))
        }
    }

    impl CompileFrom<ast::CfgNotExpr> for bool {
        fn compile(ast: &ast::CfgNotExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CfgNotExpr { arg, .. } = ast;
            Ok(!arg.as_ref().compile(env, dgns)?)
        }
    }

    impl CompileFrom<ast::CfgAnyExpr> for bool {
        fn compile(ast: &ast::CfgAnyExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CfgAnyExpr { args, .. } = ast;

            Ok(args
                .into_iter()
                .map(|e| e.compile(env, dgns))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .any(|x| x))
        }
    }

    impl CompileFrom<ast::CfgAllExpr> for bool {
        fn compile(ast: &ast::CfgAllExpr, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let ast::CfgAllExpr { args, .. } = ast;

            Ok(args
                .into_iter()
                .map(|e| e.compile(env, dgns))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .all(|x| x))
        }
    }
}

pub mod sem {
    #[derive(Default)]
    pub struct Cfg(pub Vec<String>);

    impl Cfg {
        pub fn is(&self, key: &str, val: &str) -> bool {
            self.0
                .iter()
                .rev() // Later options have higher priority
                .find_map(|x| {
                    if let Some(pos) = x.find('=') {
                        if x[..pos] == *key {
                            Some(x[(pos + 1)..] == *val)
                        } else {
                            None
                        }
                    } else if x == key {
                        Some(val == "1") // Option without argument == option with "=1"
                    } else {
                        None
                    }
                })
                .unwrap_or(val == "") // No option == option with empty string
        }

        pub fn is_on(&self, key: &str) -> bool {
            self.is(key, "1")
        }
    }
}
