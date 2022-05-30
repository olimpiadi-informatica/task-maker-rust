use std::collections::HashMap;

use by_address::ByAddress;

use crate::ir::*;
use crate::mem::*;

#[derive(Default, Debug)]
pub struct State {
    pub env: HashMap<ByAddress<Ir<DataVar>>, NodeVal>,
    pub indexes: HashMap<ByAddress<Ir<Range>>, usize>,
}

impl State {
    pub fn decl(self: &mut Self, var: &Ir<DataVar>) {
        debug_assert!(!self.env.contains_key(&var.clone().into()));
        self.env.insert(
            var.clone().into(),
            match var.ty.as_ref() {
                ExprTy::Atom { atom_ty } => NodeVal::Atom(atom_ty.sem.unwrap().cell()),
                _ => NodeVal::Array(ArrayVal::Empty),
            },
        );
    }
}
