use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use anyhow::{bail, Context, Error};
use walkdir::WalkDir;

use crate::tools::opt::ResetOpt;

/// Handler of the `reset` tool. This tool will prompt the user with a warning message and read his
/// confirmation before removing the storage directory.
///
/// Note that because some sandbox directories are read-only it's required to chmod them before
/// deleting the directory tree.
pub fn main_reset(opt: ResetOpt) -> Result<(), Error> {
    let path = opt.storage.store_dir();

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
    std::io::stdin()
        .read_line(&mut line)
        .context("Failed to read stdin")?;
    if line.trim().to_lowercase() != "y" {
        println!("Aborting...");
        return Ok(());
    }
    if !path.exists() {
        bail!("Path {} does not exist", path.display());
    }

    println!("Removing {}...", path.display());
    // first pass to make everything writable
    WalkDir::new(&path)
        .contents_first(false)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();
            if path.is_dir() {
                let mut permisions = std::fs::metadata(&path).unwrap().permissions();
                permisions.set_mode(0o755);
                if let Err(e) = std::fs::set_permissions(path, permisions) {
                    eprintln!("Failed to chmod 755 {}: {}", path.display(), e);
                }
            }
            true
        })
        .last();
    // second pass to remove everything
    if let Err(e) = std::fs::remove_dir_all(&path) {
        eprintln!("Failed to remove {}: {}", path.display(), e);
    }
    Ok(())
}
