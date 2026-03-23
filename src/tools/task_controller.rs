use anyhow::{anyhow, Context, Error};
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use task_maker_dag::{
    ControllerSettings, ExecutionGroup, ExecutionInputBehaviour, ExecutionOutputBehaviour, File,
};
use task_maker_format::{
    ui::{UIExecutionStatus, UIMessage},
    EvaluationData, SourceFile, Tag,
};
pub use task_maker_lang::GraderMap;

use crate::{ExecutionOpt, StorageOpt, ToolsSandboxRunner};

#[derive(Parser, Debug, Clone)]
pub struct TaskControllerOpt {
    /// Controller source file
    pub controller: PathBuf,

    /// Solution source file
    pub solution: PathBuf,

    /// Input file for the controller
    pub input: PathBuf,

    /// Grader/additional source file to compile with the solution
    #[clap(long)]
    pub grader: Option<PathBuf>,

    /// Maximum number of processes the controller can spawn
    #[clap(long, default_value = "200")]
    pub process_limit: usize,

    #[clap(flatten, next_help_heading = Some("EXECUTION"))]
    pub execution: ExecutionOpt,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: StorageOpt,
}

pub fn main_task_controller(opt: TaskControllerOpt) -> Result<(), Error> {
    let controller_path = opt
        .controller
        .canonicalize()
        .context("Failed to canonicalize controller path")?;
    let solution_path = opt
        .solution
        .canonicalize()
        .context("Failed to canonicalize solution path")?;
    let input_path = opt
        .input
        .canonicalize()
        .context("Failed to canonicalize input path")?;

    let (mut eval, ui_receiver) = EvaluationData::new(std::env::current_dir()?);

    // Drain the UI receiver in a separate thread and print important messages
    let ui_sender = eval.sender.clone();
    let ui_thread = std::thread::spawn(move || {
        while let Ok(msg) = ui_receiver.recv() {
            match msg {
                UIMessage::StopUI => break,
                UIMessage::Compilation { file, status } => match status {
                    UIExecutionStatus::Started { .. } => {
                        println!("Compiling {}...", file.display())
                    }
                    UIExecutionStatus::Done { result } => {
                        let res = &result[0];
                        if res.status.is_success() {
                            println!("Compilation of {}: OK", file.display());
                        } else {
                            println!(
                                "Compilation of {}: FAILED ({:?})",
                                file.display(),
                                res.status
                            );
                        }
                        if let Some(stdout) = &res.stdout {
                            let s = String::from_utf8_lossy(stdout);
                            if !s.trim().is_empty() {
                                println!("--- Compilation stdout ({}) ---", file.display());
                                println!("{}", s.trim());
                            }
                        }
                        if let Some(stderr) = &res.stderr {
                            let s = String::from_utf8_lossy(stderr);
                            if !s.trim().is_empty() {
                                println!("--- Compilation stderr ({}) ---", file.display());
                                println!("{}", s.trim());
                            }
                        }
                    }
                    UIExecutionStatus::Skipped => {
                        println!("Compilation of {}: Skipped", file.display())
                    }
                    _ => {}
                },
                UIMessage::Diagnostic { diagnostic } => {
                    println!("Diagnostic: {}", diagnostic);
                }
                UIMessage::IOIEvaluation { status, .. } => match status {
                    UIExecutionStatus::Started { worker } => {
                        println!("Evaluation started on worker {}", worker)
                    }
                    UIExecutionStatus::Done { .. } => println!("Evaluation finished"),
                    UIExecutionStatus::Skipped => println!("Evaluation skipped"),
                    _ => {}
                },
                UIMessage::ServerStatus { status } => {
                    if !status.connected_workers.is_empty() {
                        trace!(
                            "Server status: {} ready, {} waiting, {} workers",
                            status.ready_execs,
                            status.waiting_execs,
                            status.connected_workers.len()
                        );
                    }
                }
                _ => {
                    trace!("UI Message: {:?}", msg);
                }
            }
        }
    });

    let grader_map = opt
        .grader
        .as_ref()
        .map(|g| Arc::new(GraderMap::new(vec![g.clone()])));

    let controller_sf = SourceFile::new(
        &controller_path,
        controller_path.parent().unwrap(),
        "controller",
        None,
        None::<PathBuf>,
    )
    .ok_or_else(|| anyhow!("Failed to create controller SourceFile"))?;

    let solution_sf = SourceFile::new(
        &solution_path,
        solution_path.parent().unwrap(),
        "solution",
        grader_map,
        None::<PathBuf>,
    )
    .ok_or_else(|| anyhow!("Failed to create solution SourceFile"))?;

    let mut group = ExecutionGroup::new("Task controller execution");
    group.controller_settings = Some(ControllerSettings {
        process_limit: opt.process_limit,
        concurrent: true,
    });
    group.tag = Some(Tag::Evaluation.into());

    let mut controller_exec =
        controller_sf.execute(&mut eval, "controller", Vec::<String>::new())?;
    let mut solution_exec = solution_sf.execute(&mut eval, "solution", Vec::<String>::new())?;

    // provide the input file to the controller
    let input_file = File::new("input.txt");
    eval.dag.provide_file(input_file.clone(), input_path)?;

    controller_exec.input(input_file, "input.txt", false);

    controller_exec.limits_mut().wall_time = Some(10.0);
    solution_exec.stdin(ExecutionInputBehaviour::Inherit);
    solution_exec.stdout = ExecutionOutputBehaviour::Inherit;
    solution_exec.limits_mut().wall_time = Some(10.0);

    // Capture stderr to print it at the end
    let controller_stderr = controller_exec.capture_stderr(None);

    group.add_execution(controller_exec);
    group.add_execution(solution_exec);

    let group_uuid = eval.dag.add_execution_group(group);

    let controller_stderr_content = Arc::new(Mutex::new(None));
    let results_content = Arc::new(Mutex::new(None));

    {
        let c = controller_stderr_content.clone();
        eval.dag
            .get_file_content(&controller_stderr, 1024 * 1024, move |content| {
                *c.lock().unwrap() = Some(content);
                Ok(())
            });
        let c = results_content.clone();
        eval.dag.on_execution_done(&group_uuid, move |results| {
            *c.lock().unwrap() = Some(Ok(results.to_vec()));
            Ok(())
        });
        let c = results_content.clone();
        eval.dag.on_execution_skip(&group_uuid, move || {
            *c.lock().unwrap() = Some(Err(
                "Execution skipped (likely due to compilation failure)".to_string()
            ));
            Ok(())
        });
        eval.dag.on_execution_start(&group_uuid, move |worker| {
            println!("Main execution group started on worker {}", worker);
            Ok(())
        });
    }

    let store_dir = opt.storage.store_dir();
    let num_cores = opt
        .execution
        .num_cores
        .unwrap_or_else(num_cpus::get_physical);
    let sandbox_path = store_dir.join("sandboxes");

    task_maker_exec::eval_dag_locally(
        eval.dag,
        store_dir,
        num_cores,
        sandbox_path,
        opt.storage.max_cache * 1024 * 1024,
        opt.storage.min_cache * 1024 * 1024,
        ToolsSandboxRunner::default(),
    );

    // Stop the UI thread
    let _ = ui_sender.lock().unwrap().send(UIMessage::StopUI);
    let _ = ui_thread.join();

    match results_content.lock().unwrap().take() {
        Some(Ok(results)) => {
            for (i, result) in results.iter().enumerate() {
                println!(
                    "Execution {}: {:?} (cpu: {:.3}s, wall: {:.3}s, mem: {:.1}MiB)",
                    i,
                    result.status,
                    result.resources.cpu_time,
                    result.resources.wall_time,
                    result.resources.memory as f64 / 1024.0
                );
            }
        }
        Some(Err(e)) => {
            println!("Execution failed: {}", e);
        }
        None => {
            println!("No results found. The evaluation might have encountered an internal error or was interrupted.");
        }
    }

    if let Some(content) = controller_stderr_content.lock().unwrap().as_ref() {
        println!("--- Controller stderr ---");
        println!("{}", String::from_utf8_lossy(content));
    }

    Ok(())
}
