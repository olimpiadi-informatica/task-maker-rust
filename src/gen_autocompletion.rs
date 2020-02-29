//! Tool that generates the autocompletion scripts inside the target/autocompletion directory.
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use std::path::Path;
use structopt::clap::Shell;
use structopt::StructOpt;

mod opt;

fn main() {
    let mut opt = opt::Opt::clap();
    let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/autocompletion");
    std::fs::create_dir_all(&dest).unwrap();
    opt.gen_completions("task-maker-rust", Shell::Bash, &dest);
    opt.gen_completions("task-maker-rust", Shell::Zsh, &dest);
    opt.gen_completions("task-maker-rust", Shell::Fish, &dest);
    opt.gen_completions("task-maker-rust", Shell::Elvish, &dest);
    opt.gen_completions("task-maker-rust", Shell::PowerShell, &dest);
}
