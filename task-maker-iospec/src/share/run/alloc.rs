use crate::ir::*;
use crate::mem::*;

impl<'a> ExprValMut<'a> {
    pub fn alloc(self: &mut Self, expr: &Ir<DataDefExpr>, len: usize) {
        match (self, expr.ty.as_ref()) {
            (ExprValMut::Aggr(aggr), ExprTy::Array { item, .. }) => {
                debug_assert!(matches!(aggr, ArrayVal::Empty));

                match item.as_ref() {
                    ExprTy::Atom { atom_ty } => {
                        **aggr = ArrayVal::AtomArray(atom_ty.sem.unwrap().array(len))
                    }
                    ExprTy::Array { .. } => {
                        **aggr = ArrayVal::AggrArray({
                            let mut vec = Vec::with_capacity(len);
                            for _ in 0..len {
                                vec.push(ArrayVal::Empty)
                            }
                            vec
                        })
                    }
                    ExprTy::Err => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
