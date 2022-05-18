mod lit;
mod mul;
mod paren;
mod rel;
mod subscript;
mod sum;
mod var;

pub mod ast {
    use super::*;

    pub use lit::ast::*;
    pub use mul::ast::*;
    pub use paren::ast::*;
    pub use rel::ast::*;
    pub use subscript::ast::*;
    pub use sum::ast::*;
    pub use var::ast::*;
}

pub mod ir {
    use super::*;

    pub use lit::ir::*;
    pub use mul::ir::*;
    pub use paren::ir::*;
    pub use rel::ir::*;
    pub use subscript::ir::*;
    pub use sum::ir::*;
    pub use var::ir::*;
}

pub mod gen {
    use super::*;

    pub use mul::gen::*;
    pub use paren::gen::*;
    pub use rel::gen::*;
    pub use subscript::gen::*;
    pub use sum::gen::*;
    pub use var::gen::*;
}
