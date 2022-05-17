pub mod ir {
    use crate::ir::*;

    #[derive(Clone, Debug)]
    pub struct Var {
        pub name: Ir<Name>,
        pub ty: Ir<ExprTy>,
        pub kind: VarKind,
    }

    #[derive(Clone, Debug)]
    pub enum VarKind {
        Data { def: Ir<DataVar> },
        Index { range: Ir<Range> },
        Err,
    }
}

mod run {
    use crate::ir::*;
    use crate::mem::*;
    use crate::run::*;
    use crate::sem;

    impl Eval for Var {
        fn eval<'a>(
            self: &Self,
            state: &'a State,
            _ctx: &mut Context,
        ) -> Result<ExprVal<'a>, Stop> {
            Ok(match &self.kind {
                VarKind::Data { def } => {
                    match (def.ty.as_ref(), state.env.get(&def.clone().into()).unwrap()) {
                        (ExprTy::Atom { atom_ty }, NodeVal::Atom(cell)) => ExprVal::Atom(
                            cell.get(atom_ty.sem.unwrap())
                                .ok_or_else(|| -> Stop { todo!("unresolved var diagnostic") })?,
                        ),
                        (_, NodeVal::Array(ref aggr)) => ExprVal::Array(aggr),
                        _ => unreachable!(),
                    }
                }
                VarKind::Index { range } => ExprVal::Atom(sem::AtomVal::new(
                    range.bound.ty.sem.unwrap().clone(),
                    *state.indexes.get(&range.clone().into()).unwrap() as i64,
                )),
                VarKind::Err => unreachable!(),
            })
        }
    }
}
