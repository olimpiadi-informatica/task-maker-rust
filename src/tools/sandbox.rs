#[cfg(not(target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;

use anyhow::{Context, Error};
use clap::Parser;
use tabox::configuration::SandboxConfiguration;
use tabox::syscall_filter::SyscallFilter;
use tabox::{Sandbox, SandboxImplementation};
#[cfg(not(target_os = "macos"))]
use tempfile::TempDir;

#[cfg(not(target_os = "macos"))]
use task_maker_exec::sandbox::READABLE_DIRS;

#[derive(Parser, Debug, Clone)]
pub struct SandboxOpt {
    /// Working directory of the sandbox.
    ///
    /// Will be mounted in /box inside the sandbox. Defaults to current working directory.
    #[clap(long, short)]
    pub workdir: Option<PathBuf>,

    /// Memory limit for the sandbox, in KiB.
    #[clap(long, short)]
    pub memory_limit: Option<u64>,

    /// Stack limit for the sandbox, in KiB.
    #[clap(long, short)]
    pub stack_limit: Option<u64>,

    /// Prevent forking.
    #[clap(long)]
    pub single_process: bool,

    /// List of additional directory mounted read-only inside the sandbox.
    #[clap(long, short)]
    pub readable_dirs: Vec<PathBuf>,

    /// Mount /tmp and /dev/null inside the sandbox
    #[clap(long)]
    pub mount_tmpfs: bool,

    /// Mount /proc inside the sandbox
    #[clap(long)]
    pub mount_proc: bool,

    /// User id.
    #[clap(long, default_value = "1000")]
    pub uid: usize,

    /// User id.
    #[clap(long, default_value = "1000")]
    pub gid: usize,

    /// Command to execute inside the sandbox. If not specified, bash is executed.
    pub command: Vec<String>,
}

pub fn main_sandbox(opt: SandboxOpt) -> Result<(), Error> {
    let mut config = SandboxConfiguration::default();

    #[cfg(not(target_os = "macos"))]
    let _tempdir = {
        let tempdir = TempDir::new()?;
        let etcdir = tempdir.path();
        config.working_directory("/box");

        let workdir = opt
            .workdir
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| "/".into());
        config.mount(&workdir, "/box", true);

        config.mount(etcdir, "/etc", true);
        std::fs::write(
            etcdir.join("passwd"),
            format!(
                "root::0:0::/:/bin/sh\n\
                nobody::{uid}:{gid}::/:/bin/sh\n",
                uid = opt.uid,
                gid = opt.gid,
            ),
        )
        .with_context(|| format!("Failed to write /etc/etcdir in {}", etcdir.display()))?;
        std::fs::write(
            etcdir.join("group"),
            format!(
                "root:x:0:root\n\
                nobody:x:{gid}:nobody\n",
                gid = opt.gid,
            ),
        )
        .with_context(|| format!("Failed to write /etc/group in {}", etcdir.display()))?;

        for dir in READABLE_DIRS {
            if Path::new(dir).is_dir() {
                config.mount(dir, dir, false);
            }
        }
        for dir in &opt.readable_dirs {
            if dir.is_dir() {
                config.mount(dir, dir, false);
            } else {
                warn!("Cannot mount directory {}", dir.display());
            }
        }

        if opt.mount_tmpfs {
            config.mount_tmpfs(true);
        }
        if opt.mount_proc {
            config.mount_proc(true);
        }

        tempdir
    };

    config.env("PATH", std::env::var("PATH").unwrap_or_default());
    config.env("LANG", std::env::var("LANG").unwrap_or_default());
    config.env("LC_ALL", std::env::var("LC_ALL").unwrap_or_default());

    if let Some(memory_limit) = opt.memory_limit {
        config.memory_limit(memory_limit * 1024);
    }
    if let Some(stack_limit) = opt.stack_limit {
        config.stack_limit(stack_limit * 1024);
    }

    let multiproc = !opt.single_process;
    config.syscall_filter(SyscallFilter::build(multiproc, true));

    if opt.command.is_empty() {
        config.executable("/bin/bash");
    } else {
        config.executable(&opt.command[0]);
        for arg in &opt.command[1..] {
            config.arg(arg);
        }
    }

    config.uid(opt.uid);
    config.gid(opt.gid);

    debug!("Config: {:#?}", config);

    let sandbox = SandboxImplementation::run(config).context("Failed to create sandbox")?;
    let res = sandbox.wait().context("Failed to wait sandbox")?;

    debug!("Result: {:#?}", res);

    Ok(())
}
