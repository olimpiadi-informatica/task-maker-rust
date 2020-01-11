use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use failure::{bail, format_err, Error};

use task_maker_cache::Cache;
use task_maker_dag::CacheMode;
use task_maker_exec::executors::{LocalExecutor, RemoteEntityMessage};
use task_maker_exec::{connect_channel, new_local_channel, ExecutorClient};
use task_maker_format::ui::{UIMessage, UIType, UI};
use task_maker_format::UISender;
use task_maker_format::{ioi, EvaluationConfig, EvaluationData, TaskFormat};
use task_maker_store::FileStore;

use crate::error::NiceError;
use crate::opt::Opt;
use crate::sandbox::self_exec_sandbox;

/// The result of an evaluation.
pub enum Evaluation {
    /// The evaluation has completed.
    Done,
    /// The task directory has been cleaned.
    Clean,
}

/// Run the local evaluation of some actions (either building a task or cleaning its directory).
///
/// The instructions on what to do are expressed via the "command line" options passed as arguments.
/// This method will block until the execution ends, but it accepts a function as parameter used as
/// callback for when UI messages are produced.
///
/// The callback takes 2 parameters, a reference to the current UI and the message produced.
/// Typically the only thing that function should do is to send the message to the UI, this
/// behaviour can be changed.
///
/// ```no_run
/// # use structopt::StructOpt;
/// # use task_maker_rust::local::run_evaluation;
/// # let opt = task_maker_rust::opt::Opt::from_args();
/// run_evaluation(opt, move |ui, mex| ui.on_message(mex));
/// ```
pub fn run_evaluation<F>(opt: Opt, mut on_message: F) -> Result<Evaluation, Error>
where
    F: 'static + FnMut(&mut dyn UI, UIMessage) -> () + Send,
{
    if opt.exclusive {
        bail!("This option is not implemented yet");
    }

    // setup the task
    let eval_config = opt.to_config();
    let task: Box<dyn TaskFormat> = find_task(&opt.task_dir, opt.max_depth, &eval_config)
        .map_err(|e| format_err!("Invalid task directory: {}", e.to_string()))?;

    if opt.task_info {
        match opt.ui {
            UIType::Json => {
                println!("{}", serde_json::to_string(&task.task_info()?)?);
            }
            _ => {
                println!("{:#?} ", task.task_info()?);
            }
        }
        return Ok(Evaluation::Done);
    }

    // clean the task
    if opt.clean {
        task.clean()
            .map_err(|e| format_err!("Cannot clear the task directory: {}", e.to_string()))?;
        return Ok(Evaluation::Clean);
    }

    // setup the configuration and the evaluation metadata
    let (mut eval, ui_receiver) = EvaluationData::new();
    let config = eval.dag.config_mut();
    config
        .keep_sandboxes(opt.keep_sandboxes)
        .dry_run(opt.dry_run)
        .cache_mode(CacheMode::from(&opt.no_cache))
        .copy_exe(opt.copy_exe);
    if let Some(extra_time) = opt.extra_time {
        if extra_time < 0.0 {
            bail!("The extra time ({}) cannot be negative!", extra_time);
        }
        config.extra_time(extra_time);
    }

    // setup the ui thread
    let mut ui = task
        .ui(&opt.ui)
        .map_err(|_| format_err!("This UI is not supported on this task type."))?;
    let ui_thread = std::thread::Builder::new()
        .name("UI".to_owned())
        .spawn(move || {
            while let Ok(message) = ui_receiver.recv() {
                on_message(ui.as_mut(), message);
            }
            ui.finish();
        })
        .map_err(|e| format_err!("Failed to spawn UI thread: {}", e.to_string()))?;

    // setup the executor
    let store_path = opt.store_dir();
    let file_store = Arc::new(
        FileStore::new(
            store_path.join("store"),
            opt.max_cache * 1024 * 1024,
            opt.min_cache * 1024 * 1024,
        )
        .map_err(|e| format_err!("Cannot create the file store: {}", e.to_string()))?,
    );
    let cache = Cache::new(store_path.join("cache"))
        .map_err(|e| format_err!("Cannot create the cache: {}", e.to_string()))?;
    let num_cores = opt.num_cores.unwrap_or_else(num_cpus::get);
    let sandbox_path = store_path.join("sandboxes");
    let executor = LocalExecutor::new(file_store.clone(), num_cores, sandbox_path);

    // build the DAG for the task
    if let Err(e) = task.execute(&mut eval, &eval_config) {
        drop(eval.sender); // make the UI exit
        ui_thread
            .join()
            .map_err(|e| format_err!("UI panicked: {:?}", e))?;
        bail!("Cannot build task DAG! {:?}", e);
    }

    trace!("The DAG is: {:#?}", eval.dag);

    let (tx, rx, server) = if let Some(evaluate_on) = opt.evaluate_on {
        let server_addr = SocketAddr::from_str(&evaluate_on)
            .map_err(|_| format_err!("Invalid server address provided"))?;
        let (tx, rx) = connect_channel(server_addr)
            .map_err(|e| format_err!("Failed to connect to the server: {}", e.to_string()))?;
        let name = opt
            .name
            .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
        tx.send(RemoteEntityMessage::Welcome { name })
            .map_err(|e| format_err!("Cannot send welcome to the server: {}", e.to_string()))?;
        (tx.change_type(), rx.change_type(), None)
    } else {
        // start the server and the client
        let (tx, rx_remote) = new_local_channel();
        let (tx_remote, rx) = new_local_channel();
        let server = std::thread::Builder::new()
            .name("Executor thread".into())
            .spawn(move || {
                executor
                    .evaluate(tx_remote, rx_remote, cache, self_exec_sandbox)
                    .unwrap();
            })
            .map_err(|e| format_err!("Failed to spawn the executor thread: {}", e.to_string()))?;
        (tx, rx, Some(server))
    };

    let ui_sender = eval.sender.clone();
    // run the actual computation and block until it ends
    ExecutorClient::evaluate(eval.dag, tx, &rx, file_store, move |status| {
        ui_sender.send(UIMessage::ServerStatus { status })
    })
    .map_err(|e| format_err!("Client failed: {}", e.to_string()))?;

    task.sanity_check_post_hook(&mut eval.sender.lock().unwrap())
        .map_err(|e| format_err!("Sanity checks failed: {}", e.to_string()))?;

    // wait for the server and the ui to exit
    if let Some(server) = server {
        server
            .join()
            .map_err(|e| format_err!("Executor panicked: {:?}", e))?;
    }
    drop(eval.sender); // make the UI exit
    ui_thread
        .join()
        .map_err(|e| format_err!("UI panicked: {:?}", e))?;
    Ok(Evaluation::Done)
}

/// Entry point of the local execution.
pub fn main_local(opt: Opt) {
    run_evaluation(opt, |ui, mex| ui.on_message(mex))
        .nice_expect_with(|e| format!("Error: {}", e.to_string()));
}

/// Search for a valid task directory, starting from base and going _at most_ `max_depth` times up.
fn find_task<P: Into<PathBuf>>(
    base: P,
    max_depth: u32,
    eval_config: &EvaluationConfig,
) -> Result<Box<dyn TaskFormat>, Error> {
    let mut base = base.into();
    if !base.is_absolute() {
        base = getcwd().join(base);
    }
    for _ in 0..max_depth {
        if base.join("task.yaml").exists() {
            break;
        }
        base = match base.parent() {
            Some(parent) => parent.into(),
            _ => break,
        };
    }
    match ioi::Task::new(&base, eval_config) {
        Ok(task) => {
            trace!("The task is IOI: {:#?}", task);
            Ok(Box::new(task))
        }
        Err(e) => {
            warn!("Invalid task: {:?}", e);
            Err(e)
        }
    }
}

/// Return the current working directory.
///
/// `std::env::current_dir()` resolves the symlinks of the cwd's hierarchy, `$PWD` is used instead.
fn getcwd() -> PathBuf {
    std::env::var("PWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
}
