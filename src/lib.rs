//! # task-maker-rust
//!
//! This is both an application and a library, the library can be used to achieve the same
//! functionalities of the task-maker binary, inside your application.

#[macro_use]
extern crate log;

mod error;
pub mod local;
pub mod opt;
pub mod server;
pub mod worker;

pub use local::*;
pub use opt::*;
pub use server::*;
pub use worker::*;
