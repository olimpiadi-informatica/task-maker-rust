use crate::execution::*;
use crate::executor::*;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Mutex};

/// A set of utilites to schedule tasks between workers
pub struct Scheduler;

impl Scheduler {
    /// Setup the scheduler for the evaluation of a DAG.
    ///
    /// This function will lock `executor_data`, the mutex must not be held by
    /// this thread.
    pub fn setup(executor_data: Arc<Mutex<ExecutorData>>) {
        let mut data = executor_data.lock().unwrap();
        let dag = data
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
                if !dependents.contains_key(&dep) {
                    dependents.insert(dep.clone(), vec![]);
                }
                dependents.get_mut(&dep).unwrap().push(exec_uuid.clone());
            }
        }

        data.missing_deps = missing_deps;
        data.dependents = dependents;
        data.ready_execs = ready_execs;
    }

    /// Assign the most important ready jobs to the free workers.
    ///
    /// This function will lock `executor_data`, the mutex must not be held by
    /// this thread.
    pub fn schedule(executor_data: Arc<Mutex<ExecutorData>>) {
        trace!("Schedule in progress");
        let mut data = executor_data.lock().unwrap();
        let mut free_workers = vec![];
        let mut doing_workers = 0;
        for (worker_uuid, worker) in data.workers.iter() {
            if worker.job.lock().unwrap().is_none() {
                free_workers.push(worker_uuid);
            } else {
                doing_workers += 1;
            }
        }
        let mut assigned = vec![];
        let mut ready_execs = data.ready_execs.clone();
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
            data.ready_execs.len(),
            data.missing_deps.len()
        );

        for (worker, exec) in free_workers.into_iter().zip(assigned.into_iter()) {
            doing_workers += 1;
            let execution = data
                .dag
                .as_ref()
                .unwrap()
                .executions
                .get(&exec)
                .unwrap()
                .clone();
            let dep_keys = execution
                .dependencies()
                .iter()
                .map(|k| {
                    (
                        k.clone(),
                        data.file_keys
                            .get(&k)
                            .expect(&format!("Unknown file key of {}", k))
                            .clone(),
                    )
                })
                .collect();
            if data.callbacks.as_ref().unwrap().executions.contains(&exec) {
                serialize_into(
                    &ExecutorServerMessage::NotifyStart(exec.clone(), worker.clone()),
                    data.client_sender.as_ref().unwrap(),
                )
                .expect("Cannot send message to client");
            }
            Scheduler::assign_job(
                data.workers
                    .get(&worker)
                    .expect(&format!("Assigning to unknown worker {}", worker))
                    .clone(),
                WorkerJob {
                    execution,
                    dep_keys,
                },
                worker.clone(),
            );
        }

        if ready_execs.is_empty()
            && data.missing_deps.is_empty()
            && doing_workers == 0
            && data.client_sender.is_some()
        {
            serialize_into(
                &ExecutorServerMessage::Done,
                data.client_sender.as_ref().unwrap(),
            )
            .expect("Cannot send message to client");
        }
        data.ready_execs = ready_execs;
    }

    /// Mark a file as ready: ready means that the file has been correctly
    /// generated and it's present in the FileStore and all the executions that
    /// depend on the file may start if they are ready.
    ///
    /// This function will lock `executor_data`, the mutex must not be held by
    /// this thread.
    pub fn file_ready(executor_data: Arc<Mutex<ExecutorData>>, file: FileUuid) {
        trace!("File {} ready", file);
        let mut needs_reshed = false;
        {
            let mut data = executor_data.lock().unwrap();
            if !data.dependents.contains_key(&file) {
                trace!("Leaf file is ready");
                return;
            }
            let dependents = data.dependents.get(&file).unwrap().clone();
            for exec in dependents.iter() {
                if !data.missing_deps.contains_key(&exec) {
                    warn!("Invalid dependents {} of {}", exec, file);
                    continue;
                }
                let count = data.missing_deps.get_mut(&exec).unwrap();
                *count -= 1;
                if *count == 0 {
                    data.ready_execs.push(exec.clone());
                    data.missing_deps.remove(&exec);
                    needs_reshed = true;
                    trace!("Execution {} is now ready", exec);
                }
            }
        }
        if needs_reshed {
            // this call requires the lock to be free
            Scheduler::schedule(executor_data);
        } else {
            trace!("No new execution ready");
        }
    }

    /// Mark a file as failed, the generation of the file failed so all the
    /// executions that depend on this file will be skipped.
    ///
    /// This function will lock `executor_data`, the mutex must not be held by
    /// this thread.
    pub fn file_failed(executor_data: Arc<Mutex<ExecutorData>>, file: FileUuid) {
        trace!("File {} failed", file);
        let execs = {
            let mut data = executor_data.lock().unwrap();
            if !data.dependents.contains_key(&file) {
                trace!("Leaf file has failed");
                return;
            }
            let dependents = data.dependents.get(&file).unwrap().clone();
            for exec in dependents.iter() {
                data.missing_deps.remove(&exec);
                if data.callbacks.as_ref().unwrap().executions.contains(&exec) {
                    serialize_into(
                        &ExecutorServerMessage::NotifySkip(exec.clone()),
                        data.client_sender.as_ref().unwrap(),
                    )
                    .expect("Cannot send message to client");
                }
            }
            dependents
        };
        for exec in execs.iter() {
            trace!("Execution {} has been skipped", exec);
            Scheduler::exec_failed(executor_data.clone(), *exec);
        }
    }

    /// The execution failed so all its output files will not be generated.
    ///
    /// This function will lock `executor_data`, the mutex must not be held by
    /// this thread.
    pub fn exec_failed(executor_data: Arc<Mutex<ExecutorData>>, exec: ExecutionUuid) {
        let outputs = {
            let data = executor_data.lock().unwrap();
            let exec = data
                .dag
                .as_ref()
                .unwrap()
                .executions
                .get(&exec)
                .expect("Unknown execution completed");
            exec.outputs()
        };
        for output in outputs.into_iter() {
            Scheduler::file_failed(executor_data.clone(), output);
        }
    }

    /// Assign a job to the worker, waking up the thread of the executor that
    /// sends the job to the worker
    fn assign_job(worker: Arc<WorkerState>, work: WorkerJob, worker_uuid: WorkerUuid) {
        trace!("Assigning job {:?} to worker {}", work, worker_uuid);
        let mut lock = worker.job.lock().unwrap();
        *lock = Some(work);
        worker.cv.notify_one();
    }
}
