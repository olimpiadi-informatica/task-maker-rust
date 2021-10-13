use std::path::Path;

use anyhow::{anyhow, Context, Error};
use tabox::configuration::SandboxConfiguration;
use tabox::syscall_filter::SyscallFilter;
use tabox::{Sandbox, SandboxImplementation};
use tempdir::TempDir;

use task_maker_exec::sandbox::READABLE_DIRS;

use crate::tools::opt::SandboxOpt;

pub fn main_sandbox(opt: SandboxOpt) -> Result<(), Error> {
    let tempdir = TempDir::new("tm-tools-sandbox")?;
    let etcdir = tempdir.path();

    let mut config = SandboxConfiguration::default();
    config.working_directory("/box");
    let workdir = opt
        .workdir
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| "/".into());
    config.mount(&workdir, "/box", true);
    config.env("PATH", std::env::var("PATH").unwrap_or_default());

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

    let sandbox = SandboxImplementation::run(config)
        .map_err(|e| anyhow!("{}", e))
        .context("Failed to create sandbox")?;
    let res = sandbox
        .wait()
        .map_err(|e| anyhow!("{}", e))
        .context("Failed to wait sandbox")?;

    debug!("Result: {:#?}", res);

    Ok(())
}
