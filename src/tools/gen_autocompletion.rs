//! Tool that generates the autocompletion scripts inside the target/autocompletion directory.

use std::path::Path;

use anyhow::{Context, Error};
use structopt::clap::Shell;
use structopt::StructOpt;

use crate::tools::opt::GenAutocompletionOpt;

pub fn main_get_autocompletion(opt: GenAutocompletionOpt) -> Result<(), Error> {
    let target = if let Some(target) = opt.target {
        target
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/autocompletion")
    };
    let mut opt = crate::opt::Opt::clap();
    std::fs::create_dir_all(&target)
        .with_context(|| format!("Failed to create target dir: {}", target.display()))?;
    opt.gen_completions("task-maker-rust", Shell::Bash, &target);
    opt.gen_completions("task-maker-rust", Shell::Zsh, &target);
    opt.gen_completions("task-maker-rust", Shell::Fish, &target);
    opt.gen_completions("task-maker-rust", Shell::Elvish, &target);
    opt.gen_completions("task-maker-rust", Shell::PowerShell, &target);
    Ok(())
}
