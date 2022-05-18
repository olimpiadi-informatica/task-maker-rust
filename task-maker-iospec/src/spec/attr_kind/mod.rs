mod cfg;
mod doc;

pub mod ast {
    use super::*;

    pub use cfg::ast::*;
    pub use doc::ast::*;
}

pub mod kw {
    pub use super::cfg::kw::*;
    pub use super::doc::kw::*;
}

pub mod ir {
    use super::*;

    pub use cfg::ir::*;
    pub use doc::ir::*;
}

pub mod gen {
    use super::*;

    pub use cfg::gen::*;
    pub use doc::gen::*;
}
