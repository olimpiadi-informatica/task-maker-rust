use crate::proto::*;
use crate::*;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use task_maker_dag::*;

/// A set of utilities for scheduling tasks between workers.
pub(crate) struct Scheduler;

impl Scheduler {
    /// Setup the scheduler for the evaluation of a DAG.
    pub fn setup(executor_data: &mut ExecutorData) {
        let dag = executor_data
            .dag
            .as_ref()
            .expect("Setupping a scheduler without a DAG");
        let mut missing_deps = HashMap::new();
        let mut dependents = HashMap::new();
        let mut ready_execs = BinaryHeap::new();
        for (exec_uuid, exec) in dag.executions.iter() {
            let deps = exec.dependencies();
            missing_deps.insert(exec_uuid.clone(), deps.len());
            if deps.is_empty() {
                ready_execs.push(exec_uuid.clone());
            }
            for dep in deps.into_iter() {
                dependents.entry(dep).or_insert_with(|| vec![]);
                dependents.get_mut(&dep).unwrap().push(exec_uuid.clone());
            }
        }

        executor_data.missing_deps = missing_deps;
        executor_data.dependents = dependents;
        executor_data.ready_execs = ready_execs;
    }

    /// Assign the most important ready jobs to the free workers.
    pub fn schedule(executor_data: &mut ExecutorData) {
        trace!("Schedule in progress");
        let mut free_workers = vec![];
        let mut doing_workers = 0;
        for (worker_uuid, worker) in executor_data.workers.iter() {
            match *worker.job.lock().unwrap() {
                WorkerWaitingState::Waiting => free_workers.push(worker_uuid),
                WorkerWaitingState::GotJob(_) => doing_workers += 1,
                _ => {}
            }
        }
        let mut assigned = vec![];
        let mut ready_execs = executor_data.ready_execs.clone();
        while assigned.len() < free_workers.len() {
            if let Some(exec) = ready_execs.pop() {
                assigned.push(exec);
            } else {
                break;
            }
        }
        trace!(
            "{} doing workers, {} free workers, {} ready jobs, {} non-ready jobs",
            doing_workers,
            free_workers.len(),
            executor_data.ready_execs.len(),
            executor_data.missing_deps.len()
        );

        for (worker, exec) in free_workers.into_iter().zip(assigned.into_iter()) {
            doing_workers += 1;
            let execution = executor_data.dag.as_ref().unwrap().executions[&exec].clone();
            let dep_keys = execution
                .dependencies()
                .iter()
                .map(|k| {
                    (
                        *k,
                        executor_data
                            .file_handles
                            .get(&k)
                            .unwrap_or_else(|| panic!("Unknown file key of {}", k))
                            .key()
                            .clone(),
                    )
                })
                .collect();
            if executor_data
                .callbacks
                .as_ref()
                .unwrap()
                .executions
                .contains(&exec)
            {
                serialize_into(
                    &ExecutorServerMessage::NotifyStart(exec, *worker),
                    executor_data.client_sender.as_ref().unwrap(),
                )
                .expect("Cannot send message to client");
            }
            Scheduler::assign_job(
                executor_data
                    .workers
                    .get(&worker)
                    .unwrap_or_else(|| panic!("Assigning to unknown worker {}", worker))
                    .clone(),
                WorkerJob {
                    execution,
                    dep_keys,
                },
                *worker,
            );
        }

        if ready_execs.is_empty()
            && executor_data.missing_deps.is_empty()
            && doing_workers == 0
            && executor_data.client_sender.is_some()
        {
            stop_all_workers(executor_data);
            serialize_into(
                &ExecutorServerMessage::Done,
                executor_data.client_sender.as_ref().unwrap(),
            )
            .expect("Cannot send message to client");
        }
        executor_data.ready_execs = ready_execs;
    }

    /// Mark a file as ready: ready means that the file has been correctly generated and it's
    /// present in the FileStore and all the executions that depend on the file may start if they
    /// are ready.
    pub fn file_ready(executor_data: &mut ExecutorData, file: FileUuid) {
        trace!("File {} ready", file);
        let mut needs_reshed = false;
        if !executor_data.dependents.contains_key(&file) {
            trace!("Leaf file is ready");
            return;
        }
        let dependents = executor_data.dependents[&file].clone();
        for exec in dependents.iter() {
            if !executor_data.missing_deps.contains_key(&exec) {
                let exec = &executor_data.dag.as_ref().unwrap().executions[&exec];
                trace!(
                    "Cannot schedule {:?} ({}) from {}",
                    exec.description,
                    exec.uuid,
                    file
                );
                continue;
            }
            let count = executor_data.missing_deps.get_mut(&exec).unwrap();
            *count -= 1;
            if *count == 0 {
                executor_data.ready_execs.push(exec.clone());
                executor_data.missing_deps.remove(&exec);
                needs_reshed = true;
                let exec = &executor_data.dag.as_ref().unwrap().executions[&exec];
                trace!(
                    "Execution {} ({}) is now ready",
                    exec.description,
                    exec.uuid
                );
            }
        }
        if needs_reshed {
            // this call requires the lock to be free
            Scheduler::schedule(executor_data);
        } else {
            trace!("No new execution ready");
        }
    }

    /// Mark a file as failed, the generation of the file failed so all the executions that depend
    /// on this file will be skipped.
    pub fn file_failed(executor_data: &mut ExecutorData, file: FileUuid) {
        trace!("File {} failed", file);
        if !executor_data.dependents.contains_key(&file) {
            trace!("Leaf file has failed");
            return;
        }
        let dependents = executor_data.dependents[&file].clone();
        for exec in dependents.iter() {
            executor_data.missing_deps.remove(&exec);
            if executor_data
                .callbacks
                .as_ref()
                .unwrap()
                .executions
                .contains(&exec)
            {
                serialize_into(
                    &ExecutorServerMessage::NotifySkip(*exec),
                    executor_data.client_sender.as_ref().unwrap(),
                )
                .expect("Cannot send message to client");
            }
        }
        for exec in dependents.iter() {
            {
                let exec = &executor_data.dag.as_ref().unwrap().executions[&exec];
                trace!(
                    "Execution {} ({}) has been skipped due to {}",
                    exec.description,
                    exec.uuid,
                    file
                );
            }
            Scheduler::exec_failed(executor_data, *exec);
        }
    }

    /// The execution failed so all its output files will not be generated.
    pub fn exec_failed(executor_data: &mut ExecutorData, exec: ExecutionUuid) {
        let exec = executor_data
            .dag
            .as_ref()
            .unwrap()
            .executions
            .get(&exec)
            .expect("Unknown execution completed");
        for output in exec.outputs().into_iter() {
            Scheduler::file_failed(executor_data, output);
        }
    }

    /// Assign a job to the worker, waking up the thread of the executor that sends the job to the
    /// worker.
    fn assign_job(worker: Arc<WorkerState>, work: WorkerJob, worker_uuid: WorkerUuid) {
        trace!("Assigning job {:?} to worker {}", work, worker_uuid);
        let mut lock = worker.job.lock().unwrap();
        *lock = WorkerWaitingState::GotJob(work);
        worker.cv.notify_one();
    }
}
