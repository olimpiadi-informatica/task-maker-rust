mod call;
mod set;

pub mod ast {
    use super::*;

    pub use call::ast::*;
    pub use set::ast::*;
}

pub mod kw {
    pub use super::call::kw::*;
    pub use super::set::kw::*;
}

pub mod ir {
    use super::*;

    pub use call::ir::*;
    pub use set::ir::*;
}

pub mod gen {
    use super::*;

    pub use call::gen::*;
    pub use set::gen::*;
}
