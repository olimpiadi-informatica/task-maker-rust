pub use super::*;

mod block;
mod check;
mod for_loop;
mod if_stmt;
mod io;
mod item;

pub mod kw {
    use super::*;

    pub use super::item::kw::item;
    pub use check::kw::*;
    pub use io::kw::*;
}

pub mod ast {
    use super::*;

    pub use block::ast::*;
    pub use check::ast::*;
    pub use for_loop::ast::*;
    pub use if_stmt::ast::*;
    pub use io::ast::*;
    pub use item::ast::*;
}

pub mod ir {
    use super::*;

    pub use block::ir::*;
    pub use check::ir::*;
    pub use for_loop::ir::*;
    pub use if_stmt::ir::*;
    pub use io::ir::*;
    pub use item::ir::*;
}

pub mod sem {
    use super::*;

    pub use io::sem::*;
}

pub mod gen {
    use super::*;

    pub use block::gen::*;
    pub use check::gen::*;
    pub use for_loop::gen::*;
    pub use if_stmt::gen::*;
    pub use io::gen::*;
    pub use item::gen::*;
}
