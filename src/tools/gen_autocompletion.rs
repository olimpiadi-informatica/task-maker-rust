//! Tool that generates the autocompletion scripts inside the target/autocompletion directory.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use clap::{Command, CommandFactory, Parser};
use clap_complete::{Generator, Shell};

#[derive(Parser, Debug)]
pub struct GenAutocompletionOpt {
    /// Where to write the autocompletion files
    #[clap(short = 't', long = "target")]
    pub target: Option<PathBuf>,
}

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
        generate(shell, crate::Opt::command(), &target, "task-maker-rust")?;
        generate(
            shell,
            crate::tools::opt::Opt::command(),
            &target,
            "task-maker-tools",
        )?;
    }
    Ok(())
}

fn generate(shell: Shell, mut command: Command, target: &Path, name: &str) -> Result<(), Error> {
    let file_name = shell.file_name(name);
    let target = target.join(file_name);
    let mut file = File::create(&target).with_context(|| {
        format!(
            "Failed to create completion for {} at {}",
            shell,
            target.display()
        )
    })?;
    clap_complete::generate(shell, &mut command, name, &mut file);
    Ok(())
}
