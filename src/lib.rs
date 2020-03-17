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

pub use local::*;
pub use opt::*;
pub use print_dag::*;
pub use sandbox::*;
pub use server::*;
pub use worker::*;

mod detect_format;
mod error;
pub mod local;
pub mod opt;
pub mod print_dag;
pub mod sandbox;
pub mod server;
pub mod worker;
