use crate::opt::Opt;
use failure::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use task_maker_cache::Cache;
use task_maker_dag::CacheMode;
use task_maker_exec::executors::{LocalExecutor, RemoteEntityMessage};
use task_maker_exec::{connect_channel, new_local_channel, serialize_into, ExecutorClient};
use task_maker_format::ui::UIMessage;
use task_maker_format::UISender;
use task_maker_format::{ioi, EvaluationConfig, EvaluationData, TaskFormat};
use task_maker_store::FileStore;

/// Entry point of the local execution.
pub fn main_local(mut opt: Opt) {
    opt.enable_log();

    if opt.exclusive {
        unimplemented!("This option is not implemented yet");
    }

    // setup the task
    let eval_config = opt.to_config();
    let task: Box<dyn TaskFormat> = match find_task(&opt.task_dir, opt.max_depth, &eval_config) {
        Ok(task) => task,
        Err(e) => {
            eprintln!("Invalid task directory: {}", e.to_string());
            std::process::exit(1);
        }
    };

    // clean the task
    if opt.clean {
        task.clean().expect("Cannot clean task directory!");
        return;
    }

    // setup the configuration and the evaluation metadata
    let (mut eval, receiver) = EvaluationData::new();
    let config = eval.dag.config_mut();
    config
        .keep_sandboxes(opt.keep_sandboxes)
        .dry_run(opt.dry_run)
        .cache_mode(CacheMode::from(&opt.no_cache))
        .copy_exe(opt.copy_exe);
    if let Some(extra_time) = opt.extra_time {
        assert!(extra_time >= 0.0, "the extra time cannot be negative");
        config.extra_time(extra_time);
    }

    // setup the ui thread
    let mut ui = task.ui(&opt.ui).expect("Invalid UI");
    let ui_thread = std::thread::Builder::new()
        .name("UI".to_owned())
        .spawn(move || {
            while let Ok(message) = receiver.recv() {
                ui.on_message(message);
            }
            ui.finish();
        })
        .expect("Failed to spawn UI thread");

    // setup the executor
    let store_path = opt.store_dir();
    let file_store =
        Arc::new(FileStore::new(store_path.join("store")).expect("Cannot create the file store"));
    let cache = Cache::new(store_path.join("cache")).expect("Cannot create the cache");
    let num_cores = opt.num_cores.unwrap_or_else(num_cpus::get);
    let sandbox_path = store_path.join("sandboxes");
    let executor = LocalExecutor::new(file_store.clone(), num_cores, sandbox_path);

    // build the DAG for the task
    if let Err(e) = task.execute(&mut eval, &eval_config) {
        drop(eval.sender); // make the UI exit
        ui_thread.join().expect("UI panicked");
        eprintln!("Cannot build task DAG!");
        eprintln!("{:?}", e);
        std::process::exit(1);
    }

    trace!("The DAG is: {:#?}", eval.dag);

    let (tx, rx, server) = if let Some(evaluate_on) = opt.evaluate_on {
        let server_addr =
            SocketAddr::from_str(&evaluate_on).expect("Invalid server address provided");
        let (tx, rx) = connect_channel(server_addr).expect("Failed to connect to the server");
        let name = opt
            .name
            .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
        serialize_into(&RemoteEntityMessage::Welcome { name }, &tx)
            .expect("Cannot send welcome to the server");
        (tx, rx, None)
    } else {
        // start the server and the client
        let (tx, rx_remote) = new_local_channel();
        let (tx_remote, rx) = new_local_channel();
        let server = std::thread::Builder::new()
            .name("Executor thread".into())
            .spawn(move || {
                executor.evaluate(tx_remote, rx_remote, cache).unwrap();
            })
            .expect("Failed to spawn the executor thread");
        (tx, rx, Some(server))
    };

    let ui_sender = eval.sender.clone();
    ExecutorClient::evaluate(eval.dag, tx, &rx, file_store, move |status| {
        ui_sender.send(UIMessage::ServerStatus { status })
    })
    .expect("Client failed");
    task.sanity_check_post_hook(&mut eval.sender.lock().unwrap())
        .expect("Sanity checks failed");

    // wait for the server and the ui to exit
    if let Some(server) = server {
        server.join().expect("Executor panicked");
    }
    drop(eval.sender); // make the UI exit
    ui_thread.join().expect("UI panicked");
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
