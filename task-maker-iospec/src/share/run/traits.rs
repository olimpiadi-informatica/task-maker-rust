use crate::mem::*;
use crate::run::*;

pub trait Run {
    fn run(self: &Self, state: &mut State, ctx: &mut Context) -> Result<(), Stop>;
}

pub trait Eval {
    fn eval<'a>(self: &Self, state: &'a State, ctx: &mut Context) -> Result<ExprVal<'a>, Stop>;
}

pub trait EvalMut {
    fn eval_mut<'a>(
        self: &Self,
        state: &'a mut State,
        ctx: &mut Context,
    ) -> Result<ExprValMut<'a>, Stop>;
}
