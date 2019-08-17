//! DAG generation and resulting structures for [`Execution`](struct.Execution.html)s and
//! [`File`](struct.File.html)s.
//!
//! A DAG is a series of executions linked together in a non-cyclical way. Every execution has a
//! list of files as dependencies, when all of them are ready the execution can start. When an
//! execution is run inside a sandbox by a worker (under some [`ExecutionLimits`](struct.ExecutionLimits.html))
//! it will produce some output files (including `stdout` and `stderr`). Those outputs can be used
//! as inputs for the next executions.
//!
//! The sandbox should also be able to limit the available resources and measure the used ones, like
//! the execution time and the used memory.
//!
//! When some events about the execution occur the client is notified via callbacks, the supported
//! ones are:
//!
//! - the start of an execution;
//! - the completion of an execution;
//! - if an execution is skipped because a required file cannot be get (the execution which should
//!   have generated it has failed);
//! - a file has been generated.
//!
//! # Example
//!
//! Creating a simple [`Execution`](struct.Execution.html) which will run `date` to get the current
//! time. That command will print to `stdout` the current date, we capture it by calling
//! [`stdout`](struct.Execution.html#method.stdout). We also bound some callbacks to the DAG, one
//! for when the execution completes, and one for when the output is ready.
//! ```
//! use task_maker_dag::{ExecutionDAG, Execution, ExecutionCommand};
//!
//! let mut dag = ExecutionDAG::new();
//! let mut exec = Execution::new("Get the date", ExecutionCommand::System("date".into()));
//! let exec_id = exec.uuid;
//! let output = exec.stdout();
//! dag.add_execution(exec);
//! dag.on_execution_done(&exec_id, |result| println!("Elapsed time: {} seconds", result.resources.cpu_time));
//! dag.get_file_content(&output, 1000, |date| println!("The date is: {}", std::str::from_utf8(&date).unwrap()));
//! ```

extern crate boxfnonce;
extern crate failure;
extern crate serde;
extern crate task_maker_store;
extern crate uuid;

mod dag;
mod execution;
mod file;

pub use dag::*;
pub use execution::*;
pub use file::*;
