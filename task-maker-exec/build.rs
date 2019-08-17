extern crate glob;

use glob::glob;
use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    if !Path::new("tmbox").exists() {
        panic!("Please clone all the submodules! tmbox is missing");
    }
    let out_dir = env::var("OUT_DIR").unwrap();
    let num_jobs = env::var("NUM_JOBS").unwrap();
    let cxx = env::var("CXX").unwrap_or("g++".to_string());
    let status = Command::new("make")
        .arg(format!("TARGET={}", out_dir))
        .arg(format!("CXX={}", cxx))
        .arg(format!("SHELL=/bin/bash"))
        .arg("-j")
        .arg(num_jobs.to_string())
        .current_dir(Path::new("tmbox"))
        .status()
        .expect("Failed to execute make!");
    assert!(status.success());
    println!("rerun-if-changed=tmbox");
    for cc in glob("tmbox/**/*").unwrap() {
        if let Ok(f) = cc {
            println!("rerun-if-changed={}", f.display());
        }
    }
}
