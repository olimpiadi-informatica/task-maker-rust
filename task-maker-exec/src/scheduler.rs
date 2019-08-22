use crate::proto::*;
use crate::*;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use task_maker_cache::CacheResult;
use task_maker_dag::*;
use task_maker_store::FileStoreKey;

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
            missing_deps.insert(exec_uuid.clone(), deps.iter().cloned().collect());
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
    pub fn schedule(executor_data: &mut ExecutorData) -> Result<(), Error> {
        trace!("Schedule in progress");

        // process all the executions that are cached. Note that this may call `schedule`
        // recursively, any state stored locally here may become outdated after the recursive call.
        Scheduler::process_cached(executor_data)?;

        let mut free_workers = vec![];
        let mut doing_workers = 0;
        for (worker_uuid, worker) in executor_data.workers.iter() {
            match *worker.job.lock().unwrap() {
                WorkerWaitingState::Waiting => free_workers.push(*worker_uuid),
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

        executor_data.ready_execs = ready_execs;

        trace!(
            "{} doing workers, {} free workers, {} ready jobs, {} non-ready jobs, assigning {} jobs",
            doing_workers,
            free_workers.len(),
            executor_data.ready_execs.len(),
            executor_data.missing_deps.len(),
            assigned.len()
        );
        if doing_workers == 0
            && !free_workers.is_empty()
            && executor_data.ready_execs.is_empty()
            && !executor_data.missing_deps.is_empty()
            && assigned.is_empty()
        {
            // this may happen while waiting the client-provided files, should not happen anytime else
            debug!(
                "Stalled!\nworkers: {:#?}\nmissing_deps: {:#?}\nready_execs:{:#?}",
                executor_data.workers, executor_data.missing_deps, executor_data.ready_execs
            );
        }

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
                    &ExecutorServerMessage::NotifyStart(exec, worker),
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
                worker,
            );
        }

        if executor_data.ready_execs.is_empty()
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
        Ok(())
    }

    /// Mark a file as ready: ready means that the file has been correctly generated and it's
    /// present in the `FileStore` and all the executions that depend on the file may start if they
    /// are ready.
    ///
    /// This will call `schedule` if there are some new ready executions.
    pub fn file_ready(executor_data: &mut ExecutorData, file: FileUuid) -> Result<(), Error> {
        trace!("File {} ready", file);
        let mut needs_reshed = false;
        if !executor_data.dependents.contains_key(&file) {
            trace!("Leaf file is ready");
            return Ok(());
        }
        let dependents = executor_data.dependents[&file].clone();
        for exec in dependents.iter() {
            if !executor_data.missing_deps.contains_key(&exec) {
                // may happen for example in the following case
                // F1 (failed) --- exec
                // file  --------/
                // F1 will remove exec from the missing deps, in this case this execution should be
                // ignored.
                continue;
            }
            let missing_deps = executor_data.missing_deps.get_mut(&exec).unwrap();
            missing_deps.remove(&file);
            // the execution is now ready
            if missing_deps.is_empty() {
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
            Scheduler::schedule(executor_data)?;
        } else {
            trace!("No new execution ready");
        }
        Ok(())
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
    fn exec_failed(executor_data: &mut ExecutorData, exec: ExecutionUuid) {
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

    /// Mark an execution as done and, if failed, mark as failed all the depending files and
    /// executions, recursively.
    ///
    /// Send to the client the message informing the completeness of the execution.
    pub fn exec_done(
        executor_data: &mut ExecutorData,
        execution: Execution,
        result: ExecutionResult,
    ) -> Result<(), Error> {
        let status = result.status.clone();
        if executor_data
            .callbacks
            .as_ref()
            .unwrap()
            .executions
            .contains(&execution.uuid)
        {
            serialize_into(
                &ExecutorServerMessage::NotifyDone(execution.uuid, result),
                executor_data.client_sender.as_ref().unwrap(),
            )?;
        }
        match status {
            ExecutionStatus::Success => {}
            _ => Scheduler::exec_failed(executor_data, execution.uuid),
        }
        Ok(())
    }

    /// Cache the result of an evaluation if the conditions are met.
    pub fn cache_exec(
        executor_data: &mut ExecutorData,
        execution: &Execution,
        result: ExecutionResult,
        outputs: HashMap<FileUuid, FileStoreKey>,
    ) {
        if !Cache::is_cacheable(&result) {
            info!(
                "Execution {} ({}) rejected from cache because of the result {:?}",
                execution.description, execution.uuid, result
            );
            return;
        }
        let file_keys = execution
            .dependencies()
            .iter()
            .map(|f| (*f, executor_data.file_handles[f].key().clone()))
            .collect();
        let stdout = execution.stdout.as_ref().map(|f| outputs[&f.uuid].clone());
        let stderr = execution.stderr.as_ref().map(|f| outputs[&f.uuid].clone());
        let outputs = execution
            .outputs
            .iter()
            .map(|(p, f)| (p.clone(), outputs[&f.uuid].clone()))
            .collect();
        executor_data
            .cache
            .insert(&execution, &file_keys, result, stdout, stderr, outputs);
    }

    /// Search between the ready tasks if there are some that are cached, mark all of them as ready
    /// and also all the produced files. This may cause the `schedule` function to be called again.
    fn process_cached(executor_data: &mut ExecutorData) -> Result<(), Error> {
        let file_keys = executor_data
            .file_handles
            .iter()
            .map(|(uuid, hdl)| (*uuid, hdl.key().clone()))
            .collect();

        let mut still_ready_execs = BinaryHeap::new();
        let mut cached = vec![];
        // This loop changes the state of the executor (implicitly removing from ready_execs), no
        // scheduling functions must be called until executor_data.ready_execs is set to the new
        // value.
        for exec in executor_data.ready_execs.iter() {
            let execution = executor_data.dag.as_ref().unwrap().executions[&exec].clone();
            let result = executor_data.cache.get(
                &execution,
                &file_keys,
                &mut executor_data.file_store.lock().unwrap(),
            );
            match result {
                CacheResult::Hit { result, outputs } => {
                    info!("Execution {} is a cache hit!", execution.uuid);
                    cached.push((execution, result, outputs));
                }
                CacheResult::Miss => {
                    still_ready_execs.push(*exec);
                }
            }
        }
        executor_data.ready_execs = still_ready_execs;
        // scheduling functions are permitted again...
        for (execution, result, mut outputs) in cached.into_iter() {
            Scheduler::exec_done(executor_data, execution.clone(), result)?;
            if let Some(hdl) = outputs.stdout {
                let uuid = execution.stdout.unwrap().uuid;
                executor_data.file_handles.insert(uuid, hdl);
                Scheduler::file_ready(executor_data, uuid)?;
            }
            if let Some(hdl) = outputs.stderr {
                let uuid = execution.stderr.unwrap().uuid;
                executor_data.file_handles.insert(uuid, hdl);
                Scheduler::file_ready(executor_data, uuid)?;
            }
            for (path, file) in execution.outputs.iter() {
                let hdl = outputs.outputs.remove(path).unwrap();
                executor_data.file_handles.insert(file.uuid, hdl);
                Scheduler::file_ready(executor_data, file.uuid)?;
            }
        }
        Ok(())
    }
}
