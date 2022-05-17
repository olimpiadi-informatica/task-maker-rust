use crate::ir::*;
use crate::run::*;

impl Run for MetaStmt {
    fn run(self: &Self, _state: &mut State, _ctx: &mut Context) -> Result<(), Stop> {
        Ok(())
    }
}
