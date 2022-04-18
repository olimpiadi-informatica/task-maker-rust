#![allow(dead_code)]
use std::{
    collections::{HashMap, HashSet},
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
};

use crate::{
    dag::{
        Execution, ExecutionFileMode, ExecutionInputFileInfo, ExecutionLimits, ExecutionPath,
        InputFilePermissions,
    },
    file_set::{ComputationOutcome, ExecutionFile, FileReadingOutcome, FileSetFile},
    server::ServerClient,
    store::{FileSetWriteHandle, StoreClient},
};
use anyhow::{bail, Error};
use futures::future::try_join_all;
use tabox::{
    configuration::SandboxConfiguration,
    result::{ExitStatus, SandboxExecutionResult},
    syscall_filter::SyscallFilter,
};
use tarpc::context;
use task_maker_dag::{ExecutionCommand, ExecutionResourcesUsage, ExecutionResult, ExecutionStatus};
use task_maker_exec::{detect_exe::detect_exe, sandbox::READABLE_DIRS};
use tempdir::TempDir;
use tokio::{
    fs::{create_dir_all, hard_link, File},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
};

const FIFO_DIR_NAME: &str = "fifos";

async fn write_to_sandbox(
    store: &StoreClient,
    file: &ExecutionInputFileInfo,
    destination: &Path,
) -> Result<(), Error> {
    create_dir_all(destination.parent().unwrap()).await?;
    let mut destination = File::create(destination).await?;
    if file.permissions == InputFilePermissions::Executable {
        destination
            .set_permissions(PermissionsExt::from_mode(0o755))
            .await?;
    }
    loop {
        let chunk = store
            .read_chunk(
                context::current(),
                file.hash,
                file.file_id.clone(),
                destination.stream_position().await? as usize,
            )
            .await??;
        match chunk {
            FileReadingOutcome::Dropped => {
                return Err(anyhow::anyhow!(
                    "Files read by the worker should always be finalized"
                ));
            }
            FileReadingOutcome::EndOfFile => {
                break;
            }
            FileReadingOutcome::Data(chunk) => {
                destination.write_all(&chunk).await?;
            }
        };
    }
    Ok(())
}

fn box_path(execution_dir: &Path, path: &ExecutionPath) -> PathBuf {
    match path {
        ExecutionPath::Stdin => execution_dir.join("stdin"),
        ExecutionPath::Stdout => execution_dir.join("stdout"),
        ExecutionPath::Stderr => execution_dir.join("stderr"),
        ExecutionPath::Path(path) => execution_dir.join("box").join(path),
    }
}

/// Directory to use inside the sandbox as the root for the evaluation.
///
/// Due to a limitation of `tabox`, under macos the sandbox is not able to mount the directories
/// well (the bind-mounts are not available), so `/box` cannot be emulated easily. To workaround
/// this limitation, only under macos the original path is kept. This leaks some information
/// about the host, but since the sandbox is pretty fake anyway this is not really a problem.
#[allow(unused_variables)]
fn box_root(boxdir: &Path) -> PathBuf {
    // TODO(veluca): this should be moved to tabox itself.
    #[cfg(not(target_os = "macos"))]
    {
        PathBuf::from("/box")
    }
    #[cfg(target_os = "macos")]
    {
        boxdir.join("box")
    }
}

/// Check that a path is a valid local executable.
///
/// To be a valid executable the file must be _a file_ and should be in a recognized executable
/// format.
fn validate_local_executable<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    // TODO(veluca): consider making an async version of this function (more precisely, of
    // detect_exe).
    let path = path.as_ref();
    if !path.is_file() {
        bail!("Executable is not a file");
    }
    let exe = detect_exe(path)?;
    if exe.is_none() {
        bail!("Invalid executable, missing shebang?");
    }
    Ok(())
}

fn prepare_sandbox_config(
    execution: Execution,
    dir: PathBuf,
    path_for_file: &HashMap<ExecutionPath, PathBuf>,
) -> Result<SandboxConfiguration, Error> {
    let boxdir = dir.join("box");
    let boxroot = box_root(&dir);
    let mut config = SandboxConfiguration::default();
    config.working_directory(&boxroot);
    // the box directory must be writable otherwise the output files cannot be written
    config.mount(&boxdir, &boxroot, true);
    config.env("PATH", std::env::var("PATH").unwrap_or_default());
    config.stdin(
        path_for_file
            .get(&ExecutionPath::Stdin)
            .cloned()
            .unwrap_or_else(|| "/dev/null".into()),
    );
    config.stdout(
        path_for_file
            .get(&ExecutionPath::Stdout)
            .cloned()
            .unwrap_or_else(|| "/dev/null".into()),
    );
    config.stderr(
        path_for_file
            .get(&ExecutionPath::Stderr)
            .cloned()
            .unwrap_or_else(|| "/dev/null".into()),
    );
    for key in execution.copy_env.iter() {
        if let Ok(value) = std::env::var(key) {
            config.env(key, value);
        }
    }
    for (key, value) in execution.env.iter() {
        config.env(key, value);
    }

    let cpu_limit = match (execution.limits.cpu_time, execution.limits.sys_time) {
        (Some(cpu), Some(sys)) => Some(cpu + sys),
        (Some(cpu), None) => Some(cpu),
        (None, Some(sys)) => Some(sys),
        (None, None) => None,
    };
    let extra_time = execution.limits.extra_time.unwrap_or_default();
    if let Some(cpu) = cpu_limit {
        let cpu = cpu + extra_time;
        config.time_limit(cpu.as_secs());
    }
    if let Some(wall) = execution.limits.wall_time {
        let wall = wall + extra_time;
        config.wall_time_limit(wall.as_secs());
    }
    if let Some(mem) = execution.limits.memory {
        config.memory_limit(mem * 1024);
    }
    if let Some(stack) = execution.limits.stack {
        config.stack_limit(stack * 1024);
    }
    let multiproc = Some(1) != execution.limits.nproc;
    config.syscall_filter(SyscallFilter::build(
        multiproc,
        !execution.constraints.read_only,
    ));
    // has to be writable for mounting stuff in it
    config.mount(boxdir.join("etc"), "/etc", true);

    for dir in READABLE_DIRS {
        if Path::new(dir).is_dir() {
            config.mount(dir, dir, false);
        }
    }
    for dir in &execution.constraints.extra_readable_dirs {
        if dir.is_dir() {
            config.mount(dir, dir, false);
        }
    }
    if execution.constraints.mount_tmpfs {
        config.mount_tmpfs(true);
    }
    if execution.constraints.mount_proc {
        config.mount_proc(true);
    }
    match &execution.command {
        ExecutionCommand::System(cmd) => {
            if let Ok(cmd) = which::which(cmd) {
                config.executable(cmd);
            } else {
                bail!("Executable {:?} not found", cmd);
            }
        }
        ExecutionCommand::Local(cmd) => {
            let host_cmd = boxdir.join("box").join(cmd);
            validate_local_executable(&host_cmd)?;
            config.executable(boxroot.join(cmd));
        }
    };
    for arg in execution.args.iter() {
        config.arg(arg);
    }
    // drop root privileges in the sandbox
    // TODO(veluca): is 1000 always the correct UID?
    config.uid(1000);
    config.gid(1000);
    Ok(config)
}

struct ExecutionInfo {
    dir: PathBuf,
    execution: Execution,
    sandbox_config: SandboxConfiguration,
}

async fn prepare_one_execution(
    store: StoreClient,
    execution: Execution,
    group_dir: PathBuf,
) -> Result<ExecutionInfo, Error> {
    // TODO(veluca): this requires execution names to be valid directory names.
    let dir = group_dir.join(&execution.name);

    let path_for_file = Mutex::new(HashMap::new());

    // Fetch all the input files and link all the FIFOs.
    try_join_all(execution.files.iter().map(|(path, file)| async {
        let local_path = box_path(&dir, &path.clone());
        if path_for_file
            .lock()
            .unwrap()
            .insert(path.clone(), local_path.clone())
            .is_some()
        {
            return Err(anyhow::anyhow!("Duplicate file specified"));
        }
        match file.clone() {
            ExecutionFileMode::Input(input) => {
                write_to_sandbox(&store, &input, &local_path).await?
            }
            ExecutionFileMode::Fifo(fifo) => {
                hard_link(&group_dir.join(FIFO_DIR_NAME).join(fifo), local_path).await?;
            }
            ExecutionFileMode::Output => {}
        };
        Ok(())
    }))
    .await?;

    // Write /etc/passwd in the sandbox root, as some software expects it to be present.
    // TODO(veluca): this also ought to be done by tabox.
    let etc = box_path(&dir, &ExecutionPath::Path(PathBuf::new().join("etc")));
    create_dir_all(&etc).await?;
    tokio::fs::write(
        etc.join("passwd"),
        "root::0:0::/:/bin/sh\nnobody::1000:1000::/:/bin/sh\n",
    )
    .await?;

    let locked_path_for_file = path_for_file.lock().unwrap();
    let sandbox_config =
        prepare_sandbox_config(execution.clone(), dir.clone(), &locked_path_for_file)?;

    Ok(ExecutionInfo {
        sandbox_config,
        execution,
        dir,
    })
}

struct ExecutionResultInfo {
    dir: PathBuf,
    execution: Execution,
    result: ExecutionResult,
}

pub fn status_and_resources(
    sandbox_status: &SandboxExecutionResult,
    limits: &ExecutionLimits,
) -> (ExecutionStatus, ExecutionResourcesUsage) {
    let resources = ExecutionResourcesUsage {
        cpu_time: sandbox_status.resource_usage.user_cpu_time,
        sys_time: sandbox_status.resource_usage.system_cpu_time,
        wall_time: sandbox_status.resource_usage.wall_time_usage,
        memory: sandbox_status.resource_usage.memory_usage / 1024,
    };

    // it's important to check those before the signals because exceeding those
    // limits may trigger a SIGKILL from the sandbox
    if let Some(cpu_time_limit) = limits.cpu_time {
        if resources.cpu_time > cpu_time_limit.as_secs_f64() {
            return (ExecutionStatus::TimeLimitExceeded, resources);
        }
    }
    if let Some(sys_time_limit) = limits.sys_time {
        if resources.sys_time > sys_time_limit.as_secs_f64() {
            return (ExecutionStatus::SysTimeLimitExceeded, resources);
        }
    }
    if let Some(wall_time_limit) = limits.wall_time {
        if resources.wall_time > wall_time_limit.as_secs_f64() {
            return (ExecutionStatus::WallTimeLimitExceeded, resources);
        }
    }
    if let Some(memory_limit) = limits.memory {
        if resources.memory > memory_limit {
            return (ExecutionStatus::MemoryLimitExceeded, resources);
        }
    }
    match sandbox_status.status {
        ExitStatus::Signal(signal) => (
            ExecutionStatus::Signal(
                signal as u32,
                sandbox_status
                    .status
                    .signal_name()
                    .unwrap_or_else(|| "unknown signal".into()),
            ),
            resources,
        ),
        ExitStatus::ExitCode(0) => (ExecutionStatus::Success, resources),
        ExitStatus::Killed => (ExecutionStatus::WallTimeLimitExceeded, resources),
        ExitStatus::ExitCode(code) => (ExecutionStatus::ReturnCode(code as u32), resources),
    }
}

async fn run_exec(tool_path: PathBuf, exec: ExecutionInfo) -> Result<ExecutionResultInfo, Error> {
    let mut sandbox_cmd = Command::new(tool_path)
        .arg("internal-sandbox")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let string_config = serde_json::to_string(&exec.sandbox_config)?;
    sandbox_cmd
        .stdin
        .as_mut()
        .unwrap()
        .write_all(string_config.as_bytes())
        .await?;
    let output = sandbox_cmd.wait_with_output().await?;

    if !output.status.success() {
        bail!(
            "Sandbox process failed: {}\n{}",
            output.status.to_string(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let sandbox_result: SandboxExecutionResult = serde_json::from_slice(&output.stdout)?;

    let was_killed = sandbox_result.status == ExitStatus::Killed;

    let (status, resources) = status_and_resources(&sandbox_result, &exec.execution.limits);

    let execution_result = ExecutionResult {
        status,
        was_killed,
        was_cached: false,
        resources,
        // These are not set here.
        stdout: None,
        stderr: None,
    };

    Ok(ExecutionResultInfo {
        dir: exec.dir,
        execution: exec.execution,
        result: execution_result,
    })
}

async fn send_file_to_store(
    store: StoreClient,
    handle: FileSetWriteHandle,
    file: FileSetFile,
    local_path: PathBuf,
) -> Result<(), Error> {
    let mut file_to_send = File::open(local_path).await?;
    let mut buffer = [0u8; 4096];

    loop {
        let n = file_to_send.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }
        store
            .append_chunk(
                context::current(),
                handle,
                file.clone(),
                buffer[..n].to_vec(),
            )
            .await??;
    }
}

async fn send_to_store(
    store: StoreClient,
    handle: FileSetWriteHandle,
    result: ExecutionResultInfo,
) -> Result<(), Error> {
    let execution = result.execution;
    let name = execution.name;

    store
        .append_chunk(
            context::current(),
            handle,
            FileSetFile::AuxiliaryFile(name.clone(), ExecutionFile::Outcome),
            bincode::serialize(&result.result)?.to_vec(),
        )
        .await??;

    try_join_all(execution.files.into_iter().map(|(path, file)| {
        let dir = result.dir.clone();
        let name = name.clone();
        let store = store.clone();
        async move {
            let local_path = box_path(&dir, &path);
            let get_store_path = |path| -> Result<FileSetFile, Error> {
                match path {
                    ExecutionPath::Stdout => {
                        Ok(FileSetFile::AuxiliaryFile(name, ExecutionFile::Stdout))
                    }
                    ExecutionPath::Stderr => {
                        Ok(FileSetFile::AuxiliaryFile(name, ExecutionFile::Stderr))
                    }
                    ExecutionPath::Path(path) => {
                        Ok(FileSetFile::AuxiliaryFile(name, ExecutionFile::File(path)))
                    }
                    ExecutionPath::Stdin => {
                        bail!("Invalid file combination: Output + Stdin");
                    }
                }
            };
            match file {
                ExecutionFileMode::Input(_) => {}
                ExecutionFileMode::Fifo(_) => {}
                ExecutionFileMode::Output => {
                    let store_path = get_store_path(path)?;
                    send_file_to_store(store, handle, store_path, local_path).await?
                }
            };
            Result::<(), Error>::Ok(())
        }
    }))
    .await?;
    Ok(())
}

pub async fn evaluate_one_job(
    id: usize,
    server: ServerClient,
    store: StoreClient,
    sandbox_path: &Path,
) -> Result<(), Error> {
    let (execution_group, options, handle) = server.get_work(context::current(), id).await?;

    // Immediately activate the handle for writing, creating a future that will resolve
    // with the return value of activate_for_writing when file creation either finishes or the
    // request is dropped.
    let store_clone = store.clone();
    let mut finished = tokio::spawn(async move {
        store_clone
            .activate_for_writing(context::current(), handle)
            .await??;
        Result::<(), Error>::Ok(())
    });

    let group_tempdir = TempDir::new_in(sandbox_path, "tm-execgroup")?;

    let group_dir = if options.keep_sandboxes {
        group_tempdir.into_path()
    } else {
        group_tempdir.path().into()
    };

    let fifo_dir = group_dir.join(FIFO_DIR_NAME);

    create_dir_all(&fifo_dir).await?;

    let all_fifos: HashSet<_> = execution_group
        .executions
        .iter()
        .flat_map(|exec| exec.files.iter())
        .filter_map(|(_, mode)| match mode {
            crate::dag::ExecutionFileMode::Fifo(p) => Some(p),
            _ => None,
        })
        .collect();

    for fifo in all_fifos {
        nix::unistd::mkfifo(&fifo_dir.join(fifo), nix::sys::stat::Mode::S_IRWXU)?;
    }

    let all_execs =
        try_join_all(
            execution_group.executions.iter().cloned().map(|execution| {
                prepare_one_execution(store.clone(), execution, group_dir.clone())
            }),
        )
        .await?;

    let tool_path = task_maker_exec::find_tools::find_tools_path();

    let run = try_join_all(
        all_execs
            .into_iter()
            .map(|exec| run_exec(tool_path.clone(), exec)),
    );

    let execution_result: Result<_, Error> = tokio::select! {
        res = &mut finished => { res??; Ok(None) }
        res = run => { Ok(Some(res?)) }
    };

    // We don't need to do anything special if the server requires cancellation,
    // as the sandboxes are marked as kill-on-drop, so we can just exit.
    let execution_result = execution_result?
        .ok_or_else(|| anyhow::anyhow!("Execution was cancelled by the server"))?;

    try_join_all(
        execution_result
            .into_iter()
            .map(|result| send_to_store(store.clone(), handle, result)),
    )
    .await?;

    store
        .append_chunk(
            context::current(),
            handle,
            FileSetFile::MainFile,
            bincode::serialize(&ComputationOutcome::Executed)?.to_vec(),
        )
        .await??;

    store
        .finalize_file_set(context::current(), handle)
        .await??;

    Ok(())
}
