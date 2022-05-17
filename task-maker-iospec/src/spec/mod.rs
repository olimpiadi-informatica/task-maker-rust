//! Implementation of each language constructs.

extern crate syn;

mod atom_ty;
mod attr;
mod cfg_expr;
mod data_def_expr;
mod data_var;
mod expr;
mod meta_stmt;
mod name;
mod range;
mod root;
mod stmt;
mod var;

pub mod ast {
    //! Abstract Syntax Tree (AST), obtained by parsing spec file syntax.

    use super::*;

    pub use attr::ast::*;
    pub use cfg_expr::ast::*;
    pub use expr::ast::*;
    pub use meta_stmt::ast::*;
    pub use name::ast::*;
    pub use root::ast::*;
    pub use stmt::ast::*;

    pub mod kw {
        //! Custom keywords

        use super::*;

        pub use attr::kw::*;
        pub use cfg_expr::kw::*;
        pub use meta_stmt::kw::*;
        pub use range::kw::*;
        pub use stmt::kw::*;
    }
}

pub mod ir {
    use super::*;

    pub use atom_ty::ir::*;
    pub use attr::ir::*;
    pub use data_def_expr::ir::*;
    pub use data_var::ir::*;
    pub use expr::ir::*;
    pub use meta_stmt::ir::*;
    pub use name::ir::*;
    pub use range::ir::*;
    pub use root::ir::*;
    pub use stmt::ir::*;
    pub use var::ir::*;
}

pub mod sem {
    //! Implementation of semantics of some constructs.

    use super::*;

    pub use atom_ty::sem::*;
    pub use cfg_expr::sem::*;
    pub use expr::sem::*;
    pub use stmt::sem::*;
}

pub mod mem {
    //! In-memory representation of data

    use super::*;

    pub use atom_ty::mem::*;
    pub use data_def_expr::mem::*;
    pub use expr::mem::*;
}

pub mod gen {
    use super::*;

    pub use attr::gen::*;
    pub use data_def_expr::gen::*;
    pub use expr::gen::*;
    pub use meta_stmt::gen::*;
    pub use name::gen::*;
    pub use stmt::gen::*;
}
