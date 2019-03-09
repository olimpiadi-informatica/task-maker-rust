use glob::glob;
use std::path::{Path, PathBuf};

mod batch;
mod common;
pub mod formats;

pub use batch::*;
pub use common::*;

/// List all the files inside `cwd` that matches a list of glob patterns. The
/// results are in the same order of the patterns.
pub fn list_files(cwd: &Path, patterns: Vec<&str>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for pattern in patterns.into_iter() {
        for file in
            glob(cwd.join(pattern).to_str().unwrap()).expect("Invalid pattern for list_files")
        {
            results.push(file.unwrap());
        }
    }
    results
}
