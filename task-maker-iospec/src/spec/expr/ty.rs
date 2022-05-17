pub mod ir {
    use crate::ir::*;

    /// IR of the type of a value (either atomic or aggregate)
    #[derive(Default, Debug)]
    pub enum ExprTy {
        Atom {
            atom_ty: Ir<AtomTy>,
        },
        Array {
            item: Ir<ExprTy>,
            range: Ir<Range>,
        },
        #[default]
        Err,
    }

    impl ExprTy {
        pub fn to_atom_ty(&self) -> Option<Ir<AtomTy>> {
            match self {
                ExprTy::Atom { atom_ty } => Some(atom_ty.clone()),
                _ => None,
            }
        }
    }
}

pub mod gen {
    use crate::gen::*;
    use crate::ir::*;

    impl Gen<Inspect> for ExprTy {
        fn gen(&self, ctx: GenContext<Inspect>) -> Result {
            match self {
                ExprTy::Atom { atom_ty, .. } => gen!(ctx, "{}" % atom_ty),
                ExprTy::Array { item, .. } => gen!(ctx, "array of {}" % item),
                ExprTy::Err => gen!(ctx, "<<invalid-type>>"),
            }
        }
    }
}
