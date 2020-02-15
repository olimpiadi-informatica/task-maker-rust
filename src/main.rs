//! The new cmsMake!
//!
//! # Installation
//! The official way for installing `task-maker-rust` is not defined yet.
//!
//! For now you should clone the repo (with `--recurse-submodules`!) and run `cargo build --release`.
//! The executable should be located at `target/release/task-maker`.
//!
//! # Usage
//!
//! ## Simple local usage
//! Run `task-maker` in the task folder to compile and run everything.
//!
//! Specifying no option all the caches are active, the next executions will be very fast, actually doing only what's needed.
//!
//! ## Disable cache
//! If you really want to repeat the execution of something provide the `--no-cache`
//! option:
//! ```bash
//! task-maker --no-cache
//! ```
//!
//! Without any options `--no-cache` won't use any caches.
//!
//! If you want, for example, just redo the evaluations (maybe for retrying the timings), use `--no-cache=evaluation`.
//! The available options for `--no-cache` can be found with `--help`.
//!
//! ## Test only a subset of solutions
//! Sometimes you only want to test only some solutions, speeding up the compilation and cleaning a bit the output:
//! ```bash
//! task-maker sol1.cpp sol2.py
//! ```
//! Note that you may or may not specify the folder of the solution (sol/ or solution/).
//! You can also specify only the prefix of the name of the solutions you want to check.
//!
//! ## Using different task directory
//! By default the task in the current directory is executed, if you want to change the task without `cd`-ing away:
//! ```bash
//! task-maker --task-dir ~/tasks/poldo
//! ```
//!
//! ## Extracting executable files
//! All the compiled files are kept in an internal folder but if you want to use them, for example to debug a solution, passing `--copy-exe` all the useful files are copied to the `bin/` folder inside the task directory.
//! ```bash
//! task-maker --copy-exe
//! ```
//!
//! ## Do not build the statement
//! If you don't want to build the statement files (and the booklet) just pass `--no-statement`.
//! ```bash
//! task-maker --no-statement
//! ```
//!
//! ## Clean the task directory
//! If you want to clean everything, for example after the contest, simply run:
//! ```bash
//! task-maker --clean
//! ```
//! This will remove the files that can be regenerated from the task directory.
//! Note that the internal cache is not pruned by this command.
//!
//! ## Remote evaluation
//! On a server (a machine accessible from clients and workers) run
//! ```bash
//! task-maker --server
//! ```
//! This will start `task-maker` in server mode, listening for connections from clients and workers
//! respectively on port 27182 and 27183.
//!
//! Then on the worker machines start a worker each with
//! ```bash
//! task-maker --worker ip_of_the_server:27183
//! ```
//! This will start a worker on that machine (using all the cores unless specified), connecting to
//! the server and executing the jobs the server assigns.
//!
//! For running a remote computation on your machine just add the `--evaluate-on` option, like:
//! ```bash
//! task-maker --evaluate-on ip_of_the_server:27182
//! ```

#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use std::io::Write;
use std::path::PathBuf;

use structopt::StructOpt;
use walkdir::WalkDir;

pub use local::{run_evaluation, Evaluation};
pub use server::main_server;
pub use worker::main_worker;

mod error;
mod local;
mod opt;
mod sandbox;
mod server;
mod worker;

fn main() {
    let mut opt = opt::Opt::from_args();
    opt.enable_log();

    // internal API: run in sandbox mode if `--sandbox` is provided
    if opt.sandbox {
        sandbox::main_sandbox();
        return;
    }

    if opt.dont_panic {
        dont_panic(opt.store_dir());
        return;
    }

    match &opt.remote {
        Some(opt::Remote::Server(server)) => {
            let server_opt = server.clone();
            server::main_server(opt, server_opt);
        }
        Some(opt::Remote::Worker(worker)) => {
            let worker_opt = worker.clone();
            worker::main_worker(opt, worker_opt);
        }
        None => {
            local::main_local(opt);
        }
    }
}

/// Handler of the `--dont-panic` flag. Passing the store directory this function will prompt the
/// user with a warning message and read his confirmation before removing the storage directory.
///
/// Note that because some sandbox directories are read-only it's required to chmod them before
/// deleting the directory tree.
fn dont_panic(path: PathBuf) {
    println!(
        "WARNING: you are going to wipe the internal storage of task-maker, doing so while \
         running another instance of task-maker can affect the other instance."
    );
    println!(
        "This will wipe the cache and all the temporary directories, the following \
         directories will be removed:"
    );
    println!(" - {}", path.display());
    print!("Are you sure? (y/n) ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        eprintln!("Failed to read stdin");
        return;
    }
    if line.trim().to_lowercase() != "y" {
        println!("Aborting...");
        return;
    }
    if !path.exists() {
        eprintln!("Path {} does not exist", path.display());
        return;
    }

    println!("Removing {}...", path.display());
    // first pass to make everything writable
    for entry in WalkDir::new(&path).contents_first(false) {
        let entry = entry.unwrap();
        let path = entry.path();
        let mut perm = entry.metadata().unwrap().permissions();
        if perm.readonly() {
            perm.set_readonly(false);
            if let Err(e) = std::fs::set_permissions(path, perm) {
                eprintln!("Failed to chmod +w {}: {}", path.display(), e);
            }
        }
    }
    // second pass to remove everything
    if let Err(e) = std::fs::remove_dir_all(&path) {
        eprintln!("Failed to remove {}: {}", path.display(), e);
    }
}
