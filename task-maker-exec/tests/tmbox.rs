use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

fn exec_tmbox<P: AsRef<Path>, S: AsRef<OsStr> + PartialEq>(dir: P, args: &[S]) -> Output {
    let tmbox_path = Path::new(env!("OUT_DIR")).join("bin").join("tmbox");
    let tmbox_path = if tmbox_path.exists() {
        tmbox_path
    } else {
        "tmbox".into()
    };
    let mut command = Command::new(tmbox_path);
    command.arg("--json");
    command.arg("--directory");
    command.arg(dir.as_ref());
    command.args(&["--env", "PATH"]);
    command.args(&["--readable-dir", "/usr"]);
    command.args(&["--readable-dir", "/bin"]);
    command.args(&["--readable-dir", "/lib"]);
    command.args(&["--readable-dir", "/lib64"]);
    if !args.iter().any(|s| s.as_ref() == "--") {
        command.arg("--");
    }
    command.args(args);
    let output = command.output().unwrap();
    eprintln!("Output: {:#?}", output);
    output
}

#[test]
fn test_tmbox_true() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let output = exec_tmbox(tmpdir.path(), &["/bin/true"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_false() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let output = exec_tmbox(tmpdir.path(), &["/bin/false"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 1);
}

#[test]
fn test_tmbox_command_not_found() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let output = exec_tmbox(tmpdir.path(), &["dosntexists"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(out.get("error").unwrap().as_bool().unwrap());
    assert!(out.get("message").is_some());
}

#[test]
fn test_tmbox_sigsegv() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import os\n\
         os.kill(os.getpid(), 11)\n",
    )
    .unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 11);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_wall_time() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import time\n\
         start = time.monotonic()\n\
         while time.monotonic() - start <= 1.0:\n\
         \ttime.sleep(0.1)\n",
    )
    .unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    assert!(out.get("wall_time").unwrap().as_f64().unwrap() >= 1.0);
}

#[test]
fn test_tmbox_cpu_time() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import time\n\
         while time.process_time() <= 1.0:\n\
         \t[i for i in range(100000)]\n",
    )
    .unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    assert!(out.get("wall_time").unwrap().as_f64().unwrap() >= 1.0);
    assert!(
        out.get("cpu_time").unwrap().as_f64().unwrap()
            + out.get("sys_time").unwrap().as_f64().unwrap()
            >= 1.0
    );
}

#[test]
fn test_tmbox_memory() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         l = list([i for i in range(10000000)])\n",
    )
    .unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    assert!(out.get("memory_usage").unwrap().as_i64().unwrap() >= 10000000 * 4 / 1024);
}

#[test]
fn test_tmbox_stdin() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import sys\n\
         assert sys.stdin.read() == 'test'\n",
    )
    .unwrap();
    std::fs::write(tmpdir.path().join("stdin"), "test").unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &[
            "--stdin",
            &tmpdir.path().join("stdin").to_string_lossy().to_string(),
            "--",
            "/usr/bin/python3",
            "x.py",
        ],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_stdout() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         print('test')\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &[
            "--stdout",
            &tmpdir.path().join("stdout").to_string_lossy().to_string(),
            "--",
            "/usr/bin/python3",
            "x.py",
        ],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    let content = std::fs::read_to_string(tmpdir.path().join("stdout")).unwrap();
    assert_eq!(content, "test\n");
}

#[test]
fn test_tmbox_stderr() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import sys\n\
         print('test', file=sys.stderr)\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &[
            "--stderr",
            &tmpdir.path().join("stderr").to_string_lossy().to_string(),
            "--",
            "/usr/bin/python3",
            "x.py",
        ],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    let content = std::fs::read_to_string(tmpdir.path().join("stderr")).unwrap();
    assert_eq!(content, "test\n");
}

#[test]
fn test_tmbox_time_limit() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import time\n\
         while time.process_time() <= 2.0:\n\
         \t[i for i in range(100000)]\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--time", "1.0", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_ne!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    let used = out.get("cpu_time").unwrap().as_f64().unwrap()
        + out.get("sys_time").unwrap().as_f64().unwrap();
    assert!((used - 1.0).abs() < 0.01);
    assert!(out.get("wall_time").unwrap().as_f64().unwrap() >= 0.9);
}

#[test]
fn test_tmbox_memory_limit() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         l = list([i for i in range(10000000)])\n", // ~38MiB
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--memory", "10240", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_ne!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    // assert!(out.get("memory_usage").unwrap().as_i64().unwrap() >= 10240); python refuses to alloc
}

#[test]
fn test_tmbox_fsize_limit() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         with open('test.txt', 'w') as f:\n\
         \tfor _ in range(10000):\n\
         \t\tf.write('x' * 1000)\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--fsize", "1024", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_ne!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
    let len = std::fs::metadata(tmpdir.path().join("test.txt"))
        .unwrap()
        .len();
    assert!(len <= 1024 * 1024);
}

#[test]
fn test_tmbox_env() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import os\n\
         assert os.getenv('test') == 'xxx'\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--env", "test=xxx", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_no_multiprocess() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import subprocess\n\
         subprocess.run(['true'])\n",
    )
    .unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_ne!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_multiprocess() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import subprocess\n\
         subprocess.run(['true'])\n",
    )
    .unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--multiprocess", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_no_chmod() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import os\n\
         os.chmod('file.txt', 777)\n",
    )
    .unwrap();
    std::fs::write(tmpdir.path().join("file.txt"), "xxx").unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_ne!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_chmod() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         import os\n\
         os.chmod('file.txt', 777)\n",
    )
    .unwrap();
    std::fs::write(tmpdir.path().join("file.txt"), "xxx").unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--allow-chmod", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_no_tmpfs() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         open('/dev/null')\n",
    )
    .unwrap();
    std::fs::write(tmpdir.path().join("file.txt"), "xxx").unwrap();

    let output = exec_tmbox(tmpdir.path(), &["/usr/bin/python3", "x.py"]);
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_ne!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_tmbox_tmpfs() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    std::fs::write(
        tmpdir.path().join("x.py"),
        "#!/usr/bin/env python3\n\
         open('/dev/null')\n",
    )
    .unwrap();
    std::fs::write(tmpdir.path().join("file.txt"), "xxx").unwrap();

    let output = exec_tmbox(
        tmpdir.path(),
        &["--mount-tmpfs", "--", "/usr/bin/python3", "x.py"],
    );
    assert!(output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!out.get("error").unwrap().as_bool().unwrap());
    assert!(!out.get("killed_by_sandbox").unwrap().as_bool().unwrap());
    assert_eq!(out.get("signal").unwrap().as_i64().unwrap(), 0);
    assert_eq!(out.get("status_code").unwrap().as_i64().unwrap(), 0);
}
