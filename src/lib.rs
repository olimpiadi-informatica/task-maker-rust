//! # task-maker-rust
//!
//! This is both an application and a library, the library can be used to achieve the same
//! functionalities of the task-maker binary, inside your application.
#![allow(dead_code)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate scopeguard;

pub use copy_dag::*;
pub use local::*;
pub use opt::*;
pub use sandbox::*;

pub mod copy_dag;
pub mod error;
pub mod local;
pub mod opt;
pub mod remote;
pub mod sandbox;
pub mod tools;
