mod kind;
mod root;

pub mod ast {
    use super::*;

    pub use kind::ast::*;
    pub use root::ast::*;
}

pub mod kw {
    use super::*;

    pub use kind::kw::*;
}

pub mod ir {
    use super::*;

    pub use kind::ir::*;
    pub use root::ir::*;
}

pub mod gen {
    use super::*;

    pub use kind::gen::*;
    pub use root::gen::*;
}
