//! # task-maker-rust
//!
//! This is both an application and a library, the library can be used to achieve the same
//! functionalities of the task-maker binary, inside your application.
#![allow(dead_code)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

pub use local::*;
pub use opt::*;
pub use server::*;
pub use worker::*;

mod detect_format;
mod error;
pub mod local;
pub mod opt;
mod sandbox;
pub mod server;
pub mod worker;
