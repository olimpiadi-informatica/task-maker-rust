use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{stdin, stdout, BufRead, BufReader, Write},
    os::fd::{AsRawFd, OwnedFd},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{channel, Sender, TryRecvError},
        Arc, Condvar, Mutex,
    },
    thread::{self, sleep, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Error, Result};
use ductile::ChannelSender;
use nix::{
    fcntl::{fcntl, FcntlArg, FdFlag},
    unistd::pipe,
};
use tabox::configuration::SandboxConfiguration;
use task_maker_dag::{
    ControllerSettings, Execution, ExecutionCommand, ExecutionInputBehaviour,
    ExecutionOutputBehaviour, ExecutionResult, FIFO_SANDBOX_DIR,
};
use task_maker_store::FileStoreKey;
use tempfile::TempDir;
use uuid::Uuid;

use crate::{
    execution_unit::{sandbox::Sandbox, ExecutionUnit, SandboxResult},
    find_tools::find_tools_path,
    proto::WorkerClientMessage,
    worker::{compute_execution_result, get_result_outputs, OutputFile, WorkerCurrentJob},
    RawSandboxResult, SandboxRunner,
};

fn controller_keeper_inner(process_limit: usize, result_dir: &Path) -> Result<()> {
    let tools_path = find_tools_path();

    // we use fifos for communication between tmr and the controller

    #[derive(Debug)]
    struct Pipes {
        read_controller_to_sol: Option<OwnedFd>,
        write_controller_to_sol: Option<OwnedFd>,
        read_sol_to_controller: Option<OwnedFd>,
        write_sol_to_controller: Option<OwnedFd>,
    }

    let mut pipes = (0..process_limit)
        .map(|_| -> Result<_> {
            // Mac OSX does not have pipe2(), so use pipe + fcntl. This is fine because there are no
            // concurrent threads yet.
            let (read_controller_to_sol, write_controller_to_sol) = pipe()?;
            let (read_sol_to_controller, write_sol_to_controller) = pipe()?;
            for f in [
                &read_sol_to_controller,
                &write_sol_to_controller,
                &write_controller_to_sol,
                &read_controller_to_sol,
            ] {
                fcntl(f.as_raw_fd(), FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
            }
            Ok(Pipes {
                read_controller_to_sol: Some(read_controller_to_sol),
                write_controller_to_sol: Some(write_controller_to_sol),
                read_sol_to_controller: Some(read_sol_to_controller),
                write_sol_to_controller: Some(write_sol_to_controller),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let (send, recv) = channel();
    let stdin_thread = thread::spawn(move || {
        let stdin = stdin().lines();
        for line in stdin {
            if send.send(line).is_err() {
                break;
            }
        }
    });

    let fds_to_send: Vec<_> = pipes
        .iter()
        .map(|p| {
            (
                p.write_controller_to_sol.as_ref().unwrap().as_raw_fd(),
                p.read_sol_to_controller.as_ref().unwrap().as_raw_fd(),
            )
        })
        .collect();

    let mut running_boxes: Vec<(usize, Child, PathBuf)> = vec![];

    let mut num_started_processes = 0;
    'event: loop {
        for i in 0..running_boxes.len() {
            if let Some(s) = running_boxes[i].1.try_wait()? {
                if !s.success() {
                    std::fs::write(
                        &running_boxes[i].2,
                        serde_json::to_string(&RawSandboxResult::Error(format!(
                            "Sandbox process failed: {}",
                            s
                        )))?,
                    )?;
                }
                println!(
                    "DONE {}: {}",
                    running_boxes[i].0,
                    running_boxes[i].2.to_string_lossy()
                );
                stdout().flush().unwrap();
                running_boxes.swap_remove(i);
                continue 'event;
            }
        }

        // Read sandbox configurations from stdin.
        let line = match recv.try_recv() {
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                if running_boxes.is_empty() {
                    break;
                } else {
                    None
                }
            }
            Ok(line) => Some(line?),
        };

        let Some(line) = line else {
            // Sleep a bit before trying to read new events.
            std::thread::sleep(Duration::from_millis(1));
            continue;
        };

        let config: SandboxConfiguration =
            serde_json::from_str(&line).context("while parsing sandbox configuration")?;
        let config = serde_json::to_string(&config).context("Failed to serialize config")?;
        let output_path = if num_started_processes == 0 {
            result_dir.join("controller")
        } else {
            result_dir.join(format!("sol{}", num_started_processes - 1))
        };

        {
            // The sandbox expects the output file to already exist, so create it.
            File::create(&output_path)?;
        }

        if num_started_processes == 0 {
            // Controller
            // The controller should inherit `write_controller_to_sol` and `read_sol_to_controller`,
            // so clear CLOEXEC for those.
            for p in pipes.iter() {
                fcntl(
                    p.write_controller_to_sol.as_ref().unwrap().as_raw_fd(),
                    FcntlArg::F_SETFD(FdFlag::empty()),
                )?;
                fcntl(
                    p.read_sol_to_controller.as_ref().unwrap().as_raw_fd(),
                    FcntlArg::F_SETFD(FdFlag::empty()),
                )?;
            }

            let cmd = Command::new(&tools_path)
                .arg("internal-sandbox")
                .arg(config)
                .arg(output_path.as_os_str())
                .spawn()
                .context("Cannot spawn the sandbox")?;
            let pid = cmd.id();
            println!("0: {pid}");
            running_boxes.push((0, cmd, output_path));
            stdout().flush().unwrap();

            // Close the FDs passed to the controller.
            for p in pipes.iter_mut() {
                p.write_controller_to_sol.take();
                p.read_sol_to_controller.take();
            }
        } else {
            let pipes = &mut pipes[num_started_processes - 1];
            let fds = fds_to_send[num_started_processes - 1];
            let mut cmd = Command::new(&tools_path);
            cmd.arg("internal-sandbox")
                .arg(config)
                .arg(output_path.as_os_str())
                .stdin(pipes.read_controller_to_sol.take().unwrap())
                .stdout(pipes.write_sol_to_controller.take().unwrap());

            // On OSX, completely severing a process from the controlling terminal
            // seems to break using tabox.
            #[cfg(not(target_os = "macos"))]
            cmd.stderr(Stdio::null());

            let child = cmd.spawn().context("Cannot spawn the sandbox")?;
            let pid = child.id();
            println!("{}: {pid} {} {}", num_started_processes, fds.0, fds.1);
            running_boxes.push((num_started_processes, child, output_path));
            stdout().flush().unwrap();
        }
        num_started_processes += 1;
    }

    stdin_thread.join().unwrap();
    Ok(())
}

/// Runs internal bookkeeping for controlled executions.
pub fn controller_keeper(process_limit: usize, result_dir: &Path) {
    controller_keeper_inner(process_limit, result_dir).unwrap();
}

#[allow(clippy::type_complexity)]
#[derive(Debug)]
struct ControllerKeeper {
    keeper: JoinHandle<()>,
    child_stdin: Mutex<Option<ChildStdin>>,
    senders_and_pids: Arc<Mutex<Vec<(Sender<RawSandboxResult>, Arc<AtomicU32>)>>>,
    pipes: Arc<(Mutex<Vec<Option<Option<(i32, i32)>>>>, Condvar)>,
}

impl ControllerKeeper {
    fn new(process_limit: usize, temp_dir: &Path, description: &str) -> Result<Self> {
        let tools_path = find_tools_path();
        let mut cmd = Command::new(&tools_path)
            .arg("internal-controller")
            .arg(process_limit.to_string())
            .arg(temp_dir.as_os_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("Cannot spawn the controller keeper")?;

        let pipes = Arc::new((Mutex::new(vec![]), Condvar::new()));
        let senders_and_pids = Arc::new(Mutex::new(Vec::<(
            Sender<RawSandboxResult>,
            Arc<AtomicU32>,
        )>::new()));

        let stdin = cmd.stdin.take().unwrap();

        let keeper = {
            let pipes = pipes.clone();
            let senders_and_pids = senders_and_pids.clone();
            let description = description.to_string();
            // Owns the child process.
            thread::Builder::new()
                .name(format!("Controller keeper for `{description}`"))
                .spawn(move || {
                    let stdout = BufReader::new(cmd.stdout.take().unwrap());
                    for line in stdout.lines() {
                        let line = line.unwrap();
                        if line.starts_with("DONE") {
                            let line = line.split_at(5).1;
                            let (num, path) = line.split_once(':').unwrap();
                            let num: usize = num.trim().parse().unwrap();
                            // try block at home
                            let outcome = (|| -> Result<RawSandboxResult> {
                                let path = PathBuf::from(path.trim());
                                let results = serde_json::from_reader(File::open(path)?)
                                    .context("Invalid output from sandbox")?;
                                Ok(results)
                            })()
                            .into();
                            if senders_and_pids.lock().unwrap()[num]
                                .0
                                .send(outcome)
                                .is_err()
                            {
                                break;
                            }
                        } else {
                            let (num, rest) = line.split_once(':').unwrap();
                            let num: usize = num.trim().parse().unwrap();
                            let mut parts = rest.split_ascii_whitespace();
                            let pid: u32 = parts.next().unwrap().parse().unwrap();
                            senders_and_pids.lock().unwrap()[num]
                                .1
                                .store(pid, Ordering::Relaxed);
                            let p = if let Some(wp) = parts.next() {
                                let rp = parts.next().unwrap();
                                Some(Some((wp.parse().unwrap(), rp.parse().unwrap())))
                            } else {
                                Some(None)
                            };
                            {
                                let mut pipes = pipes.0.lock().unwrap();
                                while pipes.len() <= num {
                                    pipes.push(None);
                                }
                                pipes[num] = p;
                            }
                            pipes.1.notify_all();
                        }
                    }
                    let _ = cmd.kill();
                })?
        };

        Ok(Self {
            keeper,
            child_stdin: Mutex::new(Some(stdin)),
            senders_and_pids,
            pipes,
        })
    }

    fn wait_for_pipes(&self, index: usize) -> Option<(i32, i32)> {
        let mut pipes = self.pipes.0.lock().unwrap();
        while pipes.len() <= index || pipes[index].is_none() {
            pipes = self.pipes.1.wait(pipes).unwrap();
        }
        pipes[index].unwrap()
    }

    fn join(self) {
        let Self { keeper, .. } = self;
        keeper.join().unwrap();
    }

    fn terminate(&self) {
        self.child_stdin.lock().unwrap().take();
    }
}

impl SandboxRunner for ControllerKeeper {
    fn run(&self, config: SandboxConfiguration, pid: Arc<AtomicU32>) -> RawSandboxResult {
        // try blocks at home
        let result = (|| -> Result<_> {
            let (sender, receiver) = channel();
            self.senders_and_pids.lock().unwrap().push((sender, pid));
            writeln!(
                self.child_stdin.lock().unwrap().as_ref().unwrap(),
                "{}",
                serde_json::to_string(&config)?
            )?;
            Ok(receiver.recv()?)
        })();
        match result {
            Ok(r) => r,
            Err(e) => RawSandboxResult::Error(e.to_string()),
        }
    }
}

fn write_to_controller_fifo() -> String {
    "write_to_controller".to_string()
}

fn read_from_controller_fifo() -> String {
    "read_from_controller".to_string()
}

#[derive(Default)]
pub(super) struct State {
    solution_result: ExecutionResult,
    controller_result: Option<ExecutionResult>,
    exited: Vec<bool>,
    outputs: HashMap<Uuid, FileStoreKey>,
    output_paths: HashMap<Uuid, OutputFile>,
}

pub(super) fn execute_controlled_job(
    controller_settings: ControllerSettings,
    current_job: Arc<Mutex<WorkerCurrentJob>>,
    sender: &ChannelSender<WorkerClientMessage>,
    sandbox_path: &Path,
    runner: Arc<dyn SandboxRunner>,
) -> Result<JoinHandle<()>, Error> {
    // We don't use the runner, but rather unconditionally use internal-sandbox.
    drop(runner);
    let fifo_dir = TempDir::new_in(sandbox_path).with_context(|| {
        format!(
            "Failed to create temporary directory in {}",
            sandbox_path.display()
        )
    })?;
    let result_dir = TempDir::new_in(sandbox_path).with_context(|| {
        format!(
            "Failed to create temporary directory in {}",
            sandbox_path.display()
        )
    })?;

    let server_asked_files = {
        let (sender, receiver) = channel();
        current_job.lock().unwrap().server_asked_files = Some(sender);
        receiver
    };

    let (group, deps) = {
        let job = current_job.lock().unwrap();
        let job = job
            .current_job
            .as_ref()
            .ok_or_else(|| anyhow!("Worker job is gone"))?;

        let group = &job.0.group;
        assert_eq!(group.fifo.len(), 0);
        assert_eq!(group.executions.len(), 2);
        for exec in &group.executions {
            assert!(!matches!(
                exec.command,
                ExecutionCommand::TypstCompilation { .. }
            ));
        }
        assert!(matches!(
            group.executions[1].stdin,
            ExecutionInputBehaviour::Inherit
        ));
        assert!(matches!(
            group.executions[1].stdout,
            ExecutionOutputBehaviour::Inherit
        ));
        assert!(matches!(
            group.executions[1].stderr,
            ExecutionOutputBehaviour::Ignored | ExecutionOutputBehaviour::Inherit
        ));
        assert_eq!(group.executions[1].output_files.len(), 0);
        (group.clone(), job.1.clone())
    };

    let dag_config = group.config.clone();
    let description = group.description.clone();
    let sol_execution = group.executions[1].clone();

    let controller_keeper = Arc::new(ControllerKeeper::new(
        controller_settings.process_limit,
        result_dir.path(),
        &description,
    )?);

    let fifos = [write_to_controller_fifo(), read_from_controller_fifo()].into_iter();

    for fifo in fifos {
        let path = fifo_dir.path().join(&fifo);
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU)
            .with_context(|| format!("Failed to create FIFO at {}", path.display()))?;
    }

    let sb_fifo = |s| PathBuf::from(FIFO_SANDBOX_DIR).join(s);

    let mut controller_execution = Execution {
        stdin: ExecutionInputBehaviour::Path(sb_fifo(write_to_controller_fifo())),
        stdout: ExecutionOutputBehaviour::Path(sb_fifo(read_from_controller_fifo())),
        ..group.executions[0].clone()
    };

    controller_execution.limits.allow_multiprocess = true;

    debug!("Controller execution: {controller_execution:?}");

    let controller_sandbox = ExecutionUnit::Sandbox(Sandbox::new(
        sandbox_path,
        &controller_execution,
        &deps,
        Some(fifo_dir.path().to_owned()),
    )?);

    current_job.lock().unwrap().current_sandboxes = Some(vec![controller_sandbox.clone()]);

    let solution_result = ExecutionResult::default();

    current_job.lock().unwrap().controller_state = Some(State {
        solution_result,
        controller_result: None,
        exited: vec![false],
        outputs: HashMap::new(),
        output_paths: HashMap::new(),
    });

    let job_should_terminate = Arc::new(AtomicBool::new(false));

    let run_sandbox = {
        let current_job = current_job.clone();
        let description = description.clone();
        let job_should_terminate = job_should_terminate.clone();
        let controller_keeper = Arc::downgrade(&controller_keeper);
        move |mut sandbox: ExecutionUnit, execution, index| {
            let result = {
                let controller_keeper = controller_keeper.upgrade().unwrap();
                match sandbox.run(&*controller_keeper, &dag_config) {
                    Ok(res) => res,
                    Err(e) => SandboxResult::Failed {
                        error: e.to_string(),
                    },
                }
            };

            let mut result = compute_execution_result(&execution, result, &sandbox);
            let is_success = result.status.is_success();

            {
                let mut job = current_job.lock().unwrap();
                let controller_state = job.controller_state.as_mut().unwrap();
                {
                    let State {
                        outputs,
                        output_paths,
                        ..
                    } = controller_state;
                    get_result_outputs(
                        &execution,
                        &sandbox,
                        outputs,
                        output_paths,
                        &mut result.status,
                    );
                }

                // Merge result with the cumulative `solution_result`.
                if index > 0 {
                    let sr = &mut controller_state.solution_result;
                    if !result.status.is_success() {
                        sr.status = result.status.clone();
                    }
                    if result.was_killed {
                        sr.was_killed = true;
                    }
                    sr.resources.cpu_time += result.resources.cpu_time;
                    sr.resources.sys_time += result.resources.sys_time;
                    if controller_settings.concurrent {
                        sr.resources.memory += result.resources.memory;
                    } else {
                        sr.resources.memory = result.resources.memory.max(sr.resources.memory);
                    }
                    if !controller_settings.concurrent {
                        sr.resources.wall_time += result.resources.wall_time;
                    } else {
                        sr.resources.wall_time =
                            result.resources.wall_time.max(sr.resources.wall_time);
                    }
                } else {
                    controller_state.controller_result = Some(result);
                }

                controller_state.exited[index] = true;
            }

            if !is_success || index == 0 {
                job_should_terminate.store(true, Ordering::Relaxed);
                if !is_success {
                    debug!("Execution {index} in {description} failed");
                } else {
                    debug!("Controller in {description} terminated");
                }
            }
        }
    };

    let controller_thread = {
        let run_sandbox = run_sandbox.clone();
        std::thread::Builder::new()
            .name(format!("Sandbox for controller for {description}"))
            .spawn(move || run_sandbox(controller_sandbox, controller_execution, 0))?
    };

    let sandbox_manager = {
        let sender = sender.clone();
        let sandbox_path = sandbox_path.to_owned();
        let description = description.clone();
        let current_job = current_job.clone();
        move || -> Result<()> {
            // Transfer ownership of the result directory to the sandbox manager.
            let _result_dir = result_dir;

            // The order matters and has to match the order in which tabox opens the fifos!
            let mut write_to_controller = OpenOptions::new()
                .write(true)
                .open(fifo_dir.path().join(write_to_controller_fifo()))?;

            let read_from_controller =
                File::open(fifo_dir.path().join(read_from_controller_fifo()))?;

            let read_from_controller = BufReader::new(read_from_controller);

            let mut next_process_index = 0;

            macro_rules! ensure_controller {
                ($condition: expr, $fmt: expr $(, $args:tt)*) => {
                    if !($condition) {
                        warn!($fmt, $($args)*);
                        // Kill the controller.
                        current_job
                            .lock()
                            .unwrap()
                            .current_sandboxes
                            .as_mut()
                            .unwrap()[0]
                            .kill();
                        break;
                    }
                };
            }

            let mut solution_threads = vec![];

            let mut last_start = Instant::now();

            // Handle requests from the controller.
            for line in read_from_controller.lines() {
                let line = line?;
                match line.as_str() {
                    "START_SOLUTION" => {
                        // Create a new sandbox.
                        let index = next_process_index;
                        next_process_index += 1;

                        debug!("Execution group `{description}` starting a new solution {index} as requested by controller");

                        ensure_controller!(
                            index < controller_settings.process_limit,
                            "controller started too many solutions!"
                        );

                        let sol_execution = sol_execution.clone();

                        let sol_sandbox =
                            ExecutionUnit::new(&sandbox_path, &sol_execution, &deps, None)?;

                        {
                            let mut job = current_job.lock().unwrap();
                            let controller_state = job.controller_state.as_mut().unwrap();

                            if job_should_terminate.load(Ordering::Relaxed) {
                                // Exit the loop without starting a solution.
                                break;
                            }

                            controller_state.exited.push(false);

                            job.current_sandboxes
                                .as_mut()
                                .unwrap()
                                .push(sol_sandbox.clone());
                        }

                        {
                            let run_sandbox = run_sandbox.clone();
                            solution_threads.push(
                                std::thread::Builder::new()
                                    .name(format!("Sandbox for solution {index} for {description}"))
                                    .spawn(move || {
                                        run_sandbox(sol_sandbox, sol_execution, next_process_index)
                                    })?,
                            );
                        }

                        let pipes = controller_keeper
                            .wait_for_pipes(next_process_index)
                            .unwrap();

                        last_start = Instant::now();

                        if let Err(e) = writeln!(write_to_controller, "{} {}", pipes.0, pipes.1) {
                            if e.kind() == std::io::ErrorKind::BrokenPipe {
                                warn!(
                                    "Controller disappeared while writing START_SOLUTION response"
                                );
                                return Ok(());
                            }
                            return Err(e).context("Writing START_SOLUTION response to controller");
                        }
                    }
                    _ => {
                        ensure_controller!(false, "Unknown command {line}");
                    }
                }
            }

            controller_keeper.terminate();

            if job_should_terminate.load(Ordering::Relaxed) {
                let mut job = current_job.lock().unwrap();
                let WorkerCurrentJob {
                    current_sandboxes,
                    controller_state,
                    ..
                } = &mut *job;

                // Kill still-running solutions if this was a failure, or if the controller exited.
                for (_, sandbox) in controller_state
                    .as_ref()
                    .unwrap()
                    .exited
                    .iter()
                    .zip(current_sandboxes.as_ref().unwrap().iter())
                    .filter(|x| !x.0)
                {
                    if let Some(time_to_wait) =
                        (last_start + Duration::from_secs(1)).checked_duration_since(Instant::now())
                    {
                        // Ensure it has been at least a second since we last started a process. This should
                        // leave enough time for the sandbox to install the signal handler.
                        sleep(time_to_wait);
                    }
                    sandbox.kill();
                }
            }

            controller_thread.join().unwrap();
            for thread in solution_threads {
                thread.join().unwrap();
            }

            Arc::into_inner(controller_keeper)
                .with_context(|| "outstanding references to controller_keeper left")?
                .join();

            let State {
                solution_result,
                controller_result,
                outputs,
                output_paths,
                ..
            } = std::mem::take(&mut current_job.lock().unwrap().controller_state).unwrap();
            let controller_result = controller_result.unwrap();

            sender
                .send(WorkerClientMessage::WorkerDone(
                    vec![controller_result, solution_result],
                    outputs.clone(),
                ))
                .context("Failed to send WorkerDone")?;

            super::finalize_job(
                current_job,
                server_asked_files,
                outputs,
                output_paths,
                &sender,
                Some(fifo_dir),
            )
        }
    };

    Ok(std::thread::Builder::new()
        .name(format!("Sandbox manager for {description}"))
        .spawn(move || {
            sandbox_manager()
                .with_context(|| format!("Sandbox group for {description} failed"))
                // FIXME: find a better way to propagate the error to the server
                .unwrap();
        })?)
}
