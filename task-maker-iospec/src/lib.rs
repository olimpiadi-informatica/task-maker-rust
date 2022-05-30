mod share;
mod spec;

pub mod lang;
pub mod tools;

pub use share::compile;
pub use share::dgns;
pub use share::run;
pub use spec::ast;
pub use spec::mem;
pub use spec::sem;

pub use codemap;

pub mod ir {
    //! Intermediate Representation (IR) of a spec.
    //!
    //! The IR has a topology similar to the AST, but it also has links from each node
    //! to any other nodes it refers to.
    //! E.g., names are resolved by introducing a link to the node where the name is defined.
    //!
    //! IR nodes only link to nodes which occur *before* in a post-order traversal of the AST.
    //! Hence, IR nodes result in a directed acyclic graph (DAG).
    //! To represent links, nodes are wrapped in `std::rc:Rc` pointers.
    //! Since there are no cycles, no `std::rc:Weak` reference is needed.
    //!
    //! The IR contains references to all the *tokens* in the original AST, and all the information
    //! needed to reconstruct the AST tree, but does not keep any reference to the tree itself.

    use super::*;

    pub use share::ir::*;
    pub use spec::ir::*;
}

pub mod gen {
    //! Code generation.

    use super::*;

    pub use share::gen::*;
    pub use spec::gen::*;
}
