use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_dir = out_dir.join("sandbox-build");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let profile = env::var_os("PROFILE").unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=task-maker-test-sandbox");

    let mut cmd = Command::new(env::var_os("CARGO").unwrap());
    cmd.arg("build")
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(manifest.join("task-maker-test-sandbox"));
    if profile == "release" {
        cmd.arg("--release");
    }
    let status = cmd.status().expect("Sandbox bin build failed");
    assert!(status.success());
    std::fs::copy(
        target_dir.join(profile).join("task-maker-test-sandbox"),
        out_dir.join("sandbox"),
    )
    .expect("Failed to copy test sandbox bin");
}
