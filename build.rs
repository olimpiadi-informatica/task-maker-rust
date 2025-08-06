use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn get_version() -> String {
    let mut cmd = Command::new("git");
    cmd.arg("describe");
    cmd.arg("--tags");
    cmd.arg("--dirty=+dirty");
    cmd.arg("--long");
    let output = cmd.output();
    let version = env!("CARGO_PKG_VERSION").to_string();
    match output {
        Ok(output) => {
            let from_git = String::from_utf8_lossy(&output.stdout);
            let from_git = from_git.trim();
            if from_git.is_empty() {
                version
            } else {
                format!("{version}\n\nRevision: {from_git}")
            }
        }
        Err(_) => version,
    }
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("missing OUT_DIR");
    let out_dir = Path::new(&out_dir);
    let version_file_path = out_dir.join("version.txt");
    let mut file = match File::create(&version_file_path) {
        Ok(file) => file,
        Err(e) => panic!(
            "Failed to create version file at {}: {}",
            version_file_path.display(),
            e
        ),
    };

    let version = get_version();
    file.write_all(version.as_bytes())
        .expect("Failed to write to version.txt");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");
}
