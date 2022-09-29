use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Error};
use clap::{Parser, ValueHint};

use task_maker_exec::ductile::ChannelSender;
use task_maker_exec::proto::ExecutorClientMessage;
use task_maker_exec::ExecutorClient;
use task_maker_format::ui::{CursesUI, StdoutPrinter, UIMessage, BLUE, BOLD, RED, UI, YELLOW};
use task_maker_format::{cwrite, cwriteln, get_sanity_check_list, EvaluationConfig};

use crate::context::RuntimeContext;
use crate::tools::find_bad_case::dag::{patch_dag, patch_task_for_batch, TestcaseData};
use crate::tools::find_bad_case::state::{SharedUIState, UIState};
use crate::{ExecutionOpt, FindTaskOpt, StorageOpt};

mod curses_ui;
mod dag;
mod finish_ui;
mod state;

#[derive(Parser, Debug, Clone)]
#[clap(trailing_var_arg = true)]
pub struct FindBadCaseOpt {
    #[clap(flatten, next_help_heading = Some("TASK SEARCH"))]
    pub find_task: FindTaskOpt,

    #[clap(flatten, next_help_heading = Some("EXECUTION"))]
    pub execution: ExecutionOpt,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: StorageOpt,

    /// Number of input files to generate for each batch.
    ///
    /// Setting this to a small value may reduce the speed of this tool.
    #[clap(long, short, default_value = "100")]
    pub batch_size: usize,

    /// Path to the solution to check against the official solution of the task.
    #[clap(value_hint = ValueHint::FilePath)]
    pub solution: PathBuf,

    /// Arguments to pass to the generator. The value '{}' will be replaced with a random seed.
    #[clap(num_args = 0..)]
    pub generator_args: Vec<String>,
}

pub fn main_find_bad_case(opt: FindBadCaseOpt) -> Result<(), Error> {
    if !opt.solution.exists() {
        bail!("Cannot find solution at {}", opt.solution.display());
    }

    let eval_config = EvaluationConfig {
        solution_filter: vec![],
        booklet_solutions: false,
        no_statement: true,
        solution_paths: vec![opt.solution.clone()],
        disabled_sanity_checks: get_sanity_check_list(),
        seed: None,
        dry_run: false,
    };
    let working_directory =
        tempfile::TempDir::new().context("Failed to create working directory")?;

    // A reference to the current executor, used for sending messages to it.
    let current_executor_sender: Arc<Mutex<Option<ChannelSender<_>>>> = Arc::new(Mutex::new(None));
    let stop_evaluation = {
        let current_executor_sender = current_executor_sender.clone();
        move || {
            let current_executor_sender = current_executor_sender.lock().unwrap();
            if let Some(sender) = current_executor_sender.as_ref() {
                let _ = sender.send(ExecutorClientMessage::Stop);
            }
        }
    };

    let task = opt.find_task.find_task(&eval_config)?;
    let task_path = task.path().to_path_buf();

    // Create a single UI for all the batches.
    let ui_state = UIState::new(&opt, stop_evaluation);
    let shared_state = ui_state.shared.clone();
    let mut ui = CursesUI::<UIState, curses_ui::CursesUI, finish_ui::FinishUI>::new(ui_state)
        .context("Failed to start Curses UI")?;

    // Start the global UI where all the messages will be sent.
    let (sender, receiver) = std::sync::mpsc::channel();
    let global_ui_join_handle = std::thread::Builder::new()
        .name("Global UI".into())
        .spawn(move || {
            while let Ok(Some(message)) = receiver.recv() {
                ui.on_message(message);
            }
            ui.finish();
        })
        .expect("Failed to start UI thread");

    // Bind the ctrl-c handler that will make the UI and the executor stop.
    ctrlc::set_handler({
        let shared_state = shared_state.clone();
        let current_executor_sender = current_executor_sender.clone();
        move || {
            shared_state.write().unwrap().should_stop = true;
            let current_executor_sender = current_executor_sender.lock().unwrap();
            if let Some(sender) = current_executor_sender.as_ref() {
                if sender.send(ExecutorClientMessage::Stop).is_err() {
                    error!("Cannot tell the server to stop");
                }
            }
        }
    })
    .context("Failed to set ctrl-c handler")?;

    for batch_index in 0.. {
        let mut task = opt.find_task.find_task(&eval_config)?;
        let batch = patch_task_for_batch(
            &mut task,
            &opt.generator_args,
            opt.batch_size,
            batch_index,
            working_directory.path(),
        )?;

        {
            let mut shared_state = shared_state.write().unwrap();
            shared_state.last_batch = Some(batch.clone());
            shared_state.batch_index = batch_index;
        }

        // Setup the configuration and the evaluation metadata.
        let context = RuntimeContext::new(task, &opt.execution, |task, eval| {
            task.build_dag(eval, &eval_config)
                .context("Cannot build the task DAG")?;
            patch_dag(eval, opt.batch_size, &batch).context("Cannot patch the DAG")
        })?;

        let mut executor = context.connect_executor(&opt.execution, &opt.storage)?;

        let ui_receiver = executor.ui_receiver;
        let ui_thread = std::thread::Builder::new()
            .name("UI".to_owned())
            .spawn({
                let sender = sender.clone();
                move || {
                    while let Ok(message) = ui_receiver.recv() {
                        if let UIMessage::StopUI = message {
                            break;
                        }
                        let _ = sender.send(Some(message));
                    }
                }
            })
            .context("Failed to spawn UI thread")?;

        let mut dag = executor.eval.dag.clone();
        std::mem::swap(&mut dag, &mut executor.eval.dag);

        // Run the actual computation and block until it ends.
        let sender = sender.clone();
        *current_executor_sender.lock().unwrap() = Some(executor.tx.clone());
        ExecutorClient::evaluate(
            dag,
            executor.tx,
            &executor.rx,
            executor.file_store,
            move |status| {
                sender
                    .send(Some(UIMessage::ServerStatus { status }))
                    .map_err(|e| anyhow!("{:?}", e))
            },
        )
        .with_context(|| {
            shared_state.write().unwrap().should_stop = true;
            "Client failed"
        })?;

        // Disable the ctrl-c handler dropping the owned clone of the sender, letting the client exit.
        current_executor_sender.lock().unwrap().take();

        drop(executor.eval);
        drop(executor.task);
        drop(executor.rx);

        if let Some(local_executor) = executor.local_executor {
            local_executor
                .join()
                .map_err(|e| anyhow!("Executor panicked: {:?}", e))
                .unwrap()
                .expect("Local executor failed");
        }
        ui_thread
            .join()
            .map_err(|e| anyhow!("UI panicked: {:?}", e))
            .unwrap();

        if shared_state.read().unwrap().should_stop {
            break;
        }
    }

    let _ = sender.send(None);
    global_ui_join_handle
        .join()
        .map_err(|e| anyhow!("{:?}", e))
        .context("Global UI thread failed")?;

    let mut printer = StdoutPrinter::default();

    let shared_state = shared_state.read().unwrap();
    let (testcase, message) = match shared_state.failing_testcase.clone() {
        Some(testcase) => testcase,
        None => {
            cwriteln!(printer, YELLOW, "No bad case found");
            print_failures(&shared_state, &mut printer);
            return Ok(());
        }
    };
    let (input_path, correct_output_path, failing_output_path) =
        copy_testcase(&testcase, &task_path)?;

    cwrite!(printer, BOLD, "Solution:           ");
    println!("{}", opt.solution.display());
    cwrite!(printer, BOLD, "Batch size:         ");
    println!("{}", opt.batch_size);

    cwriteln!(printer, BOLD, "Failed testcase:");
    cwrite!(printer, BOLD, "    Generator args: ");
    println!("{}", testcase.generator_args.join(" "));
    cwrite!(printer, BOLD, "    Seed:           ");
    println!("{}", testcase.seed);
    cwrite!(printer, BOLD, "    Message:        ");
    println!("{}", message);
    println!();
    print_file("Input file", &task_path, &input_path, &mut printer)?;
    if let Some(correct_output_path) = correct_output_path {
        print_file(
            "Correct output file",
            &task_path,
            &correct_output_path,
            &mut printer,
        )?;
    }
    if let Some(failing_output_path) = failing_output_path {
        print_file(
            "Failing output file",
            &task_path,
            &failing_output_path,
            &mut printer,
        )?;
    }

    print_failures(&shared_state, &mut printer);
    Ok(())
}

fn copy_testcase(
    testcase: &TestcaseData,
    task_path: &Path,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>), Error> {
    let target_dir = task_path.join(format!("fuzz/bad-cases/seed-{}", testcase.seed));
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("Failed to create {}", target_dir.display()))?;

    let input_target = target_dir.join("input.txt");
    let correct_output_target = target_dir.join("correct-output.txt");
    let failing_output_target = target_dir.join("failing-output.txt");

    std::fs::copy(&testcase.input_path, &input_target).with_context(|| {
        format!(
            "Failed to copy {} -> {}",
            testcase.input_path.display(),
            input_target.display()
        )
    })?;
    // FIXME: the output files may not be produced, or not be present in the write_to because we
    //        stop the execution before it ends normally. This means that some executions may be
    //        skipped and their output not produced.
    let correct_output_target = if testcase.correct_output_path.exists() {
        std::fs::copy(&testcase.correct_output_path, &correct_output_target).with_context(
            || {
                format!(
                    "Failed to copy {} -> {}",
                    testcase.correct_output_path.display(),
                    correct_output_target.display()
                )
            },
        )?;
        Some(correct_output_target)
    } else {
        None
    };
    let failing_output_target = if testcase.output_path.exists() {
        std::fs::copy(&testcase.output_path, &failing_output_target).with_context(|| {
            format!(
                "Failed to copy {} -> {}",
                testcase.output_path.display(),
                failing_output_target.display()
            )
        })?;
        Some(failing_output_target)
    } else {
        warn!("Output file not found, maybe the solution was killed");
        None
    };

    Ok((input_target, correct_output_target, failing_output_target))
}

fn print_file(
    title: &str,
    base_path: &Path,
    path: &Path,
    printer: &mut StdoutPrinter,
) -> Result<(), Error> {
    cwrite!(printer, BLUE, "{} ", title);
    println!(
        "(at {})",
        path.strip_prefix(base_path).unwrap_or(path).display()
    );
    let file = std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let content = String::from_utf8_lossy(&file);
    const MAX_CONTENT_LEN: usize = 256;
    if content.len() > MAX_CONTENT_LEN {
        println!("{}...\n", &content[..MAX_CONTENT_LEN].trim_end());
    } else {
        println!("{}\n", content.trim_end());
    }
    Ok(())
}

fn print_failures(shared: &SharedUIState, printer: &mut StdoutPrinter) {
    if let Some((testcase, message, result)) = &shared.errored_testcase {
        println!();
        cwrite!(printer, RED, "Error: ");
        println!("{}", message);
        cwrite!(printer, BOLD, "Generator args: ");
        println!("{}", testcase.generator_args.join(" "));
        cwrite!(printer, BOLD, "Result:         ");
        println!("{:?}", result.status);
        if let Some(stderr) = &result.stderr {
            cwriteln!(printer, BOLD, "Stderr:");
            println!("{}", String::from_utf8_lossy(stderr));
        }
    }
}
