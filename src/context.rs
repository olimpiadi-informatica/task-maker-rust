//! This mod exposes an interface for setting up and running an execution.
//!
//! The structs here follow a multi-step builder pattern, moving from a struct to the next adding
//! more and more context.
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anyhow::{anyhow, bail, Context, Error};

use task_maker_cache::Cache;
use task_maker_dag::CacheMode;
use task_maker_exec::ductile::{new_local_channel, ChannelReceiver, ChannelSender};
use task_maker_exec::executors::{LocalExecutor, RemoteEntityMessage, RemoteEntityMessageResponse};
use task_maker_exec::proto::{ExecutorClientMessage, ExecutorServerMessage};
use task_maker_exec::ExecutorClient;
use task_maker_format::ui::{UIChannelReceiver, UIMessage, UIType, UI};
use task_maker_format::{EvaluationData, TaskFormat, UISender, VALID_TAGS};
use task_maker_store::FileStore;

use crate::remote::connect_to_remote_server;
use crate::{render_dag, ExecutionOpt, StorageOpt, ToolsSandboxRunner};

/// Version of task-maker.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// First step of the execution: take a task and build the Execution DAG. This needs setting the
/// first configurations of the environment.
pub struct RuntimeContext {
    pub task: TaskFormat,
    pub eval: EvaluationData,
    pub ui_receiver: UIChannelReceiver,
    pub sandbox_runner: ToolsSandboxRunner,
}

/// Second step: connect to an executor (either local or remote). This opens the local store and
/// setups the local executor if necessary.
pub struct ConnectedExecutor {
    // fields from RuntimeContext
    pub task: TaskFormat,
    pub eval: EvaluationData,
    pub ui_receiver: UIChannelReceiver,

    // new fields
    pub file_store: Arc<FileStore>,
    pub tx: ChannelSender<ExecutorClientMessage>,
    pub rx: ChannelReceiver<ExecutorServerMessage>,
    pub local_executor: Option<JoinHandle<Result<(), Error>>>,
}

/// Third step: start the UI thread.
pub struct ConnectedExecutorWithUI {
    // fields from ConnectedExecutor
    pub task: TaskFormat,
    pub eval: EvaluationData,
    pub file_store: Arc<FileStore>,
    pub tx: ChannelSender<ExecutorClientMessage>,
    pub rx: ChannelReceiver<ExecutorServerMessage>,
    pub local_executor: Option<JoinHandle<Result<(), Error>>>,

    // new fields
    pub ui_thread: JoinHandle<()>,
    pub client_sender: Arc<Mutex<Option<ChannelSender<ExecutorClientMessage>>>>,
}

impl RuntimeContext {
    /// Create a [`RuntimeContext`] for the given task. In the provided closure you should build the
    /// execution DAG for the execution. The closure is given a reference to the given task and a
    /// reference to the evaluation data.
    pub fn new<BuildDag>(
        mut task: TaskFormat,
        opt: &ExecutionOpt,
        build_dag: BuildDag,
    ) -> Result<Self, Error>
    where
        BuildDag: FnOnce(&mut TaskFormat, &mut EvaluationData) -> Result<(), Error>,
    {
        let (mut eval, ui_receiver) = EvaluationData::new(task.path());

        // extract the configuration from the command line arguments
        let config = eval.dag.config_mut();
        config
            .keep_sandboxes(opt.keep_sandboxes)
            .dry_run(opt.dry_run)
            .cache_mode(
                CacheMode::try_from(&opt.no_cache, &VALID_TAGS).context("Invalid cache mode")?,
            )
            .copy_exe(opt.copy_exe)
            .copy_logs(opt.copy_logs)
            .priority(opt.priority);
        if let Some(extra_time) = opt.extra_time {
            if extra_time < 0.0 {
                bail!("The extra time ({}) cannot be negative!", extra_time);
            }
            config.extra_time(extra_time);
        }
        if let Some(extra_memory) = opt.extra_memory {
            config.extra_memory(extra_memory);
        }

        // build the execution dag
        build_dag(&mut task, &mut eval)?;

        trace!("The DAG is: {:#?}", eval.dag);
        if opt.copy_dag {
            let dot = render_dag(&eval.dag);
            let bin = task.path().join("bin");
            std::fs::create_dir_all(&bin).context("Failed to create bin/ directory")?;
            std::fs::write(bin.join("DAG.dot"), dot).context("Failed to write bin/DAG.dot")?;
        }

        Ok(Self {
            task,
            eval,
            ui_receiver,
            sandbox_runner: ToolsSandboxRunner::default(),
        })
    }

    /// Change the default sandbox runner for the local executor.
    pub fn sandbox_runner(&mut self, sandbox_runner: ToolsSandboxRunner) {
        self.sandbox_runner = sandbox_runner;
    }

    /// Start the local executor or connect to a remote one.
    pub fn connect_executor(
        self,
        opt: &ExecutionOpt,
        storage_opt: &StorageOpt,
    ) -> Result<ConnectedExecutor, Error> {
        // setup the file store
        let store_path = storage_opt.store_dir();
        let file_store = Arc::new(
            FileStore::new(
                store_path.join("store"),
                storage_opt.max_cache * 1024 * 1024,
                storage_opt.min_cache * 1024 * 1024,
            )
            .context(
                "Cannot create the file store (You can try wiping it with task-maker-tools reset)",
            )?,
        );

        // connect either to the remote executor or spawn a local one
        let (tx, rx, local_executor) = if let Some(evaluate_on) = &opt.evaluate_on {
            let (tx, rx) = connect_to_remote_server(evaluate_on, 27182)
                .context("Cannot connect to the remote server")?;
            let name = opt
                .name
                .clone()
                .unwrap_or_else(|| format!("{}@{}", whoami::username(), whoami::hostname()));
            tx.send(RemoteEntityMessage::Welcome {
                name,
                version: VERSION.into(),
            })
            .context("Cannot send welcome to the server")?;
            if let RemoteEntityMessageResponse::Rejected(err) =
                rx.recv().context("Failed to receive welcome response")?
            {
                bail!("The server rejected the client connection: {}", err);
            }
            (tx.change_type(), rx.change_type(), None)
        } else {
            // start the server and the client
            let (tx, rx_remote) = new_local_channel();
            let (tx_remote, rx) = new_local_channel();

            // setup the local cache
            let cache_path = store_path.join("cache");
            let cache = Cache::new(cache_path).context("Cannot create the cache")?;

            // setup the local executor
            let num_cores = opt.num_cores.unwrap_or_else(num_cpus::get);
            let sandbox_path = storage_opt.store_dir().join("sandboxes");
            let executor = LocalExecutor::new(
                file_store.clone(),
                cache,
                num_cores,
                sandbox_path,
                self.sandbox_runner,
            )?;
            let local_executor = std::thread::Builder::new()
                .name("Executor thread".into())
                .spawn(move || executor.evaluate(tx_remote, rx_remote))
                .context("Failed to spawn the executor thread")?;
            (tx, rx, Some(local_executor))
        };

        Ok(ConnectedExecutor {
            task: self.task,
            eval: self.eval,
            ui_receiver: self.ui_receiver,

            file_store,
            tx,
            rx,
            local_executor,
        })
    }
}

impl ConnectedExecutor {
    /// Now that we are connected to an executor, we can start the UI thread in background. This
    /// thread will run until the execution is completed or until it is stopped.
    ///
    /// The callback takes 2 parameters, a reference to the current UI and the message produced.
    pub fn start_ui<OnMessage>(
        mut self,
        ui_type: &UIType,
        mut on_message: OnMessage,
    ) -> Result<ConnectedExecutorWithUI, Error>
    where
        OnMessage: FnMut(&mut dyn UI, UIMessage) + Send + 'static,
    {
        let config = self.eval.dag.config_mut().clone();
        // setup the UI thread
        let mut ui = self
            .task
            .ui(ui_type, config)
            .context("This UI is not supported on this task type")?;
        let ui_receiver = self.ui_receiver;
        let ui_thread = std::thread::Builder::new()
            .name("UI".to_owned())
            .spawn(move || {
                while let Ok(message) = ui_receiver.recv() {
                    if let UIMessage::StopUI = message {
                        break;
                    }
                    on_message(ui.as_mut(), message);
                }
                ui.finish();
            })
            .context("Failed to spawn UI thread")?;

        // a shared sender for the ctrl-c handler, it has to be wrapped in Arc-Mutex-Option to be freed
        // at the end of the computation to allow the client to exit.
        let client_sender = Arc::new(Mutex::new(Some(self.tx.clone())));
        // `ctrlc` crate doesn't allow multiple calls of set_handler, and the tests may call this
        // function multiple times, so in the tests ^C handler is disabled.
        #[cfg(not(test))]
        {
            let client_sender = client_sender.clone();
            if let Err(e) = ctrlc::set_handler(move || {
                let sender = client_sender.lock().unwrap();
                if let Some(sender) = sender.as_ref() {
                    if sender.send(ExecutorClientMessage::Stop).is_err() {
                        error!("Cannot tell the server to stop");
                    }
                }
            }) {
                warn!("Cannot bind control-C handler: {:?}", e);
            }
        }

        Ok(ConnectedExecutorWithUI {
            task: self.task,
            eval: self.eval,
            file_store: self.file_store,
            tx: self.tx,
            rx: self.rx,
            local_executor: self.local_executor,

            ui_thread,
            client_sender,
        })
    }
}

impl ConnectedExecutorWithUI {
    /// Finally, start the execution and wait until it ends or it is stopped.
    pub fn execute(mut self) -> Result<(), Error> {
        let ui_sender = self.eval.sender.clone();
        // Create a copy of the DAG, keeping the cloned object inside the EvaluationData, while the
        // original is stored in `dag`. This because after cloning a ExecutionDAG the copies don't
        // have access to the callbacks.
        let mut dag = self.eval.dag.clone();
        std::mem::swap(&mut dag, &mut self.eval.dag);

        let local_executor = self.local_executor;
        let ui_thread = self.ui_thread;
        let sender = self.eval.sender.clone();
        defer! {
            // wait for the executor and the ui to exit
            if let Some(local_executor) = local_executor {
                local_executor
                    .join()
                    .map_err(|e| anyhow!("Executor panicked: {:?}", e))
                    .unwrap()
                    .expect("Local executor failed");
            }
            let _ = sender.send(UIMessage::StopUI);
            ui_thread
                .join()
                .map_err(|e| anyhow!("UI panicked: {:?}", e))
                .unwrap();
        }

        // run the actual computation and block until it ends
        let client_sender = self.client_sender;
        ExecutorClient::evaluate(dag, self.tx, &self.rx, self.file_store, move |status| {
            ui_sender.send(UIMessage::ServerStatus { status })
        })
        .with_context(|| {
            if let Some(tx) = client_sender.lock().unwrap().as_ref() {
                let _ = tx.send(ExecutorClientMessage::Stop);
            }
            "Client failed"
        })?;
        // disable the ctrl-c handler dropping the owned clone of the sender, letting the client exit
        client_sender.lock().unwrap().take();

        self.task
            .sanity_check_post_hook(&mut self.eval)
            .context("Sanity checks failed")?;
        Ok(())
    }
}
