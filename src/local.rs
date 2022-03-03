use anyhow::{bail, Context, Error};

use task_maker_format::ui::{UIMessage, UI};

use crate::context::RuntimeContext;
use crate::error::NiceError;
use crate::opt::Opt;

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
pub fn run_evaluation<F>(opt: Opt, on_message: F) -> Result<Evaluation, Error>
where
    F: FnMut(&mut dyn UI, UIMessage) + Send + 'static,
{
    if opt.exclusive {
        bail!("This option is not implemented yet");
    }

    // setup the task
    let eval_config = opt.to_config();
    let task = opt.find_task.find_task(&eval_config)?;

    // clean the task
    if opt.clean {
        warn!("--clean is deprecated: use `task-maker-tools clear`");
        task.clean().context("Cannot clear the task directory")?;
        return Ok(Evaluation::Clean);
    }

    // setup the configuration and the evaluation metadata
    let context = RuntimeContext::new(task, &opt.execution, |task, eval| {
        // build the DAG for the task
        task.build_dag(eval, &eval_config)
            .context("Cannot build the task DAG")
    })?;

    // start the execution
    let executor = context.connect_executor(&opt.execution, &opt.storage)?;
    let executor = executor.start_ui(&opt.ui.ui, on_message)?;
    executor.execute()?;

    Ok(Evaluation::Done)
}

/// Entry point of the local execution.
pub fn main_local(opt: Opt) {
    run_evaluation(opt, |ui, mex| ui.on_message(mex)).nice_unwrap();
}
