pub use super::*;

mod kind;
mod root;
mod ty;

pub mod ast {
    use super::*;

    pub use kind::ast::*;
    pub use root::ast::*;
}

pub mod ir {
    use super::*;

    pub use kind::ir::*;
    pub use root::ir::*;
    pub use ty::ir::*;
}

pub mod sem {
    use super::*;

    pub use root::sem::*;
}

pub mod mem {
    use super::*;

    pub use root::mem::*;
}

pub mod gen {
    use super::*;

    pub use kind::gen::*;
    pub use root::gen::*;
}
