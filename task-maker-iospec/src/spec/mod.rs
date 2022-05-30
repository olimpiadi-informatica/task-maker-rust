//! Implementation of each language constructs.

extern crate syn;

mod atom_ty;
mod attr;
mod attr_kind;
mod stmt_block;
mod cfg_expr;
mod data_def_expr;
mod data_var;
mod expr;
mod expr_kind;
mod expr_ty;
mod meta_stmt;
mod meta_stmt_kind;
mod name;
mod range;
mod root;
mod stmt;
mod stmt_kind;
mod var;

pub mod ast {
    //! Abstract Syntax Tree (AST), obtained by parsing spec file syntax.

    use super::*;

    pub use attr::ast::*;
    pub use attr_kind::ast::*;
    pub use stmt_block::ast::*;
    pub use cfg_expr::ast::*;
    pub use expr::ast::*;
    pub use expr_kind::ast::*;
    pub use meta_stmt::ast::*;
    pub use meta_stmt_kind::ast::*;
    pub use name::ast::*;
    pub use root::ast::*;
    pub use stmt::ast::*;
    pub use stmt_kind::ast::*;

    pub mod kw {
        //! Custom keywords

        use super::*;

        pub use attr_kind::kw::*;
        pub use cfg_expr::kw::*;
        pub use meta_stmt_kind::kw::*;
        pub use range::kw::*;
        pub use stmt_kind::kw::*;
    }
}

pub mod ir {
    use super::*;

    pub use atom_ty::ir::*;
    pub use attr::ir::*;
    pub use attr_kind::ir::*;
    pub use stmt_block::ir::*;
    pub use data_def_expr::ir::*;
    pub use data_var::ir::*;
    pub use expr::ir::*;
    pub use expr_kind::ir::*;
    pub use expr_ty::ir::*;
    pub use meta_stmt::ir::*;
    pub use meta_stmt_kind::ir::*;
    pub use name::ir::*;
    pub use range::ir::*;
    pub use root::ir::*;
    pub use stmt::ir::*;
    pub use stmt_kind::ir::*;
    pub use var::ir::*;
}

pub mod sem {
    //! Implementation of semantics of some constructs.

    use super::*;

    pub use atom_ty::sem::*;
    pub use cfg_expr::sem::*;
    pub use expr::sem::*;
    pub use stmt_kind::sem::*;
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
    pub use attr_kind::gen::*;
    pub use stmt_block::gen::*;
    pub use data_def_expr::gen::*;
    pub use expr::gen::*;
    pub use expr_kind::gen::*;
    pub use expr_ty::gen::*;
    pub use meta_stmt_kind::gen::*;
    pub use name::gen::*;
    pub use stmt::gen::*;
    pub use stmt_kind::gen::*;
}
