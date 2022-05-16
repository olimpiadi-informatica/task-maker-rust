//! Tool that generates the autocompletion scripts inside the target/autocompletion directory.

use std::fs::File;
use std::path::Path;

use crate::Opt;
use anyhow::{Context, Error};
use clap::CommandFactory;
use clap_complete::{Generator, Shell};

use crate::tools::opt::GenAutocompletionOpt;

pub fn main_get_autocompletion(opt: GenAutocompletionOpt) -> Result<(), Error> {
    let target = if let Some(target) = opt.target {
        target
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/autocompletion")
    };
    std::fs::create_dir_all(&target)
        .with_context(|| format!("Failed to create target dir: {}", target.display()))?;
    for shell in [
        Shell::Bash,
        Shell::Zsh,
        Shell::Fish,
        Shell::Elvish,
        Shell::PowerShell,
    ] {
        let file_name = shell.file_name("task-maker-rust");
        let target = target.join(file_name);
        let mut file = File::create(&target).with_context(|| {
            format!(
                "Failed to create completion for {} at {}",
                shell,
                target.display()
            )
        })?;
        let mut opt = Opt::command();
        clap_complete::generate(shell, &mut opt, "task-maker-rust", &mut file);
    }
    Ok(())
}
