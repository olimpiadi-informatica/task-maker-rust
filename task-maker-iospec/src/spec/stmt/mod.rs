mod block;
mod kind;
mod root;

pub mod kw {
    use super::*;

    pub use kind::kw::*;
}

pub mod ast {
    use super::*;

    pub use block::ast::*;
    pub use kind::ast::*;
    pub use root::ast::*;
}

pub mod ir {
    use super::*;

    pub use block::ir::*;
    pub use kind::ir::*;
    pub use root::ir::*;
}

pub mod sem {
    use super::*;

    pub use kind::sem::*;
}

pub mod gen {
    use super::*;

    pub use block::gen::*;
    pub use kind::gen::*;
    pub use root::gen::*;
}
