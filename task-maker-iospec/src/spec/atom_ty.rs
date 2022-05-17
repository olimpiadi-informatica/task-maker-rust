pub mod ir {
    use super::sem;
    use crate::ir::*;

    /// IR of the type of an atomic value
    #[derive(Default, Debug)]
    pub struct AtomTy {
        pub kind: AtomTyKind,
        pub sem: Option<sem::AtomTy>,
    }

    #[derive(Default, Debug)]
    pub enum AtomTyKind {
        Name {
            name: Ir<Name>,
        },
        Lit {
            token: syn::LitInt,
        },
        /// Result of a comparison
        Rel {
            rels: Vec<RelExpr>,
        },
        #[default]
        Err,
    }

    pub struct ExprList<'a>(pub &'a Vec<Ir<Expr>>);
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;
    use crate::sem;

    impl<L> Gen<CommonMixin<'_, L>> for AtomTy {
        fn gen(&self, ctx: GenContext<CommonMixin<'_, L>>) -> Result {
            match &self.sem {
                Some(ty) => match ty {
                    sem::AtomTy::Bool => gen!(ctx, "bool"),
                    sem::AtomTy::I32 => gen!(ctx, "i32"),
                    sem::AtomTy::I64 => gen!(ctx, "i64"),
                },
                _ => gen!(ctx, "<<compile-error>>"),
            }
        }
    }

    lang_mixin!(Inspect, AtomTy, CommonMixin);
}

pub mod sem {
    use std::fmt;
    use std::str::FromStr;

    /// Semantics of the type of an atomic value
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AtomTy {
        Bool,
        I32,
        I64,
    }

    use AtomTy::*;

    impl AtomTy {
        pub fn all() -> Vec<Self> {
            vec![Bool, I32, I64]
        }

        pub fn name(self: Self) -> String {
            match self {
                Bool => "bool".into(),
                I32 => "i32".into(),
                I64 => "i64".into(),
            }
        }

        pub fn value_range(self: Self) -> (i64, i64) {
            match self {
                Bool => (0, 1),
                I32 => (i32::min_value() as i64 + 1, i32::max_value() as i64),
                I64 => (i64::min_value() as i64 + 1, i64::max_value() as i64),
            }
        }
    }

    impl FromStr for AtomTy {
        type Err = ();

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Self::all().into_iter().find(|k| &k.name() == s).ok_or(())
        }
    }

    impl fmt::Display for AtomTy {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.name())
        }
    }
}

pub mod mem {
    use num_traits::Bounded;
    use num_traits::Num;
    use num_traits::NumCast;
    use std::fmt::Debug;

    use crate::sem;

    pub type AtomVal = sem::AtomVal;

    pub trait Empty {
        fn empty() -> Self;
    }

    impl<T: Bounded> Empty for T {
        fn empty() -> Self {
            Self::min_value()
        }
    }

    /// Compact representation of an atom, to use in array cells
    trait AtomMem: Clone + Copy + Debug + Num + Empty + NumCast {}

    impl AtomMem for u8 {}
    impl AtomMem for i32 {}
    impl AtomMem for i64 {}

    pub trait AtomCell: Debug {
        fn get(self: &Self, ty: sem::AtomTy) -> Option<AtomVal>;
        fn set(self: &mut Self, value: AtomVal);
    }

    impl<T: AtomMem> AtomCell for T {
        fn get(self: &Self, ty: sem::AtomTy) -> Option<AtomVal> {
            if *self == Self::empty() {
                None
            } else {
                Some(AtomVal::new(ty, (*self).to_i64().unwrap()))
            }
        }

        fn set(self: &mut Self, value: AtomVal) {
            *self = <T as NumCast>::from(value.value_i64()).unwrap()
        }
    }

    pub trait AtomArray: Debug {
        fn at(self: &Self, index: usize) -> &dyn AtomCell;
        fn at_mut(self: &mut Self, index: usize) -> &mut dyn AtomCell;
    }

    impl<T: AtomMem> AtomArray for Vec<T> {
        fn at(self: &Self, index: usize) -> &dyn AtomCell {
            &self[index]
        }

        fn at_mut(self: &mut Self, index: usize) -> &mut dyn AtomCell {
            &mut self[index]
        }
    }

    impl sem::AtomTy {
        pub fn cell(self: &Self) -> Box<dyn AtomCell> {
            match self {
                sem::AtomTy::Bool => Box::new(u8::empty()),
                sem::AtomTy::I32 => Box::new(i32::empty()),
                sem::AtomTy::I64 => Box::new(i64::empty()),
            }
        }

        pub fn array(self: &Self, len: usize) -> Box<dyn AtomArray> {
            match self {
                sem::AtomTy::Bool => Box::new(vec![u8::empty(); len]),
                sem::AtomTy::I32 => Box::new(vec![i32::empty(); len]),
                sem::AtomTy::I64 => Box::new(vec![i64::empty(); len]),
            }
        }
    }
}

mod compile {
    use std::str::FromStr;

    use crate::ast;
    use crate::compile::*;
    use crate::ir::*;
    use crate::sem;

    impl CompileFrom<ast::Name> for AtomTy {
        fn compile(ast: &ast::Name, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
            let name: Ir<Name> = ast.compile(env, dgns)?;
            let sem = sem::AtomTy::from_str(&name.ident.to_string());

            if sem.is_err() {
                dgns.error(
                    &format!("invalid scalar type `{}`", name.ident.to_string()),
                    vec![dgns.error_ann("invalid type", name.span())],
                    vec![dgns.help_footer(&format!(
                        "supported types are {}",
                        sem::AtomTy::all()
                            .iter()
                            .map(|ty| ty.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))],
                );
            }

            Ok(Self {
                sem: sem.ok(),
                kind: AtomTyKind::Name { name },
            })
        }
    }

    impl AnalyzeFrom<Expr> for Option<Ir<AtomTy>> {
        fn analyze(ir: &Expr, dgns: &mut DiagnosticContext) -> Self {
            match ir.ty.as_ref() {
                ExprTy::Atom { atom_ty } => Some(atom_ty.clone()),
                _ => {
                    dgns.error(
                        &format!("expected a scalar type, got `{}`", quote_hir(&ir.ty),),
                        vec![dgns.error_ann("not a scalar", ir.span())],
                        vec![],
                    );
                    Default::default()
                }
            }
        }
    }

    impl AnalyzeFrom<ExprList<'_>> for Option<Ir<AtomTy>> {
        fn analyze(ir: &ExprList, dgns: &mut DiagnosticContext) -> Self {
            let scalars: Vec<_> =
                ir.0.iter()
                    .flat_map(|factor| {
                        let ty: Option<Ir<AtomTy>> = factor.analyze(dgns);
                        ty.and_then(|ty| ty.sem.map(|ty_sem| (factor, ty, ty_sem)))
                    })
                    .collect();

            let (first, ty, ty_sem) = match scalars.first() {
                Some(x) => x,
                _ => {
                    return Default::default();
                }
            };

            let mismatched_type = scalars.iter().find(|(_, _, ty_sem2)| ty_sem2 != ty_sem);

            match mismatched_type {
                Some((expr, actual_ty, _)) => {
                    dgns.error(
                        &format!(
                            "expected type `{}`, got `{}`",
                            quote_hir(ty),
                            quote_hir(actual_ty)
                        ),
                        vec![
                            dgns.error_ann(&format!("expected `{}`", quote_hir(ty)), expr.span()),
                            dgns.info_ann(&format!("this is a `{}`", quote_hir(ty)), first.span()),
                            dgns.info_ann("actual type here", actual_ty.span()),
                            dgns.info_ann("expected type here", ty.span()),
                        ],
                        vec![],
                    );
                    Default::default()
                }
                None => Some(ty.clone()),
            }
        }
    }
}

mod dgns {
    use super::ir::*;
    use crate::dgns::*;

    impl HasSpan for AtomTy {
        fn span(self: &Self) -> Span {
            match &self.kind {
                AtomTyKind::Name { name } => name.span(),
                AtomTyKind::Lit { token } => token.span(),
                // FIXME: duplicate code
                AtomTyKind::Rel { rels } => rels
                    .first()
                    .unwrap()
                    .0
                    .span()
                    .join(rels.last().unwrap().2.span())
                    .unwrap(),
                AtomTyKind::Err => panic!(),
            }
        }
    }
}
