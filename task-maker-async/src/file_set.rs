use std::{collections::HashMap, path::PathBuf};

use futures::Future;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::{channel, error::RecvError, Sender};

/// Identifier for a file in an execution.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum ExecutionFile {
    Outcome, // serialized task_maker_dag::ExecutionResult, with stdout/stderr set to None.
    Stdout,
    Stderr,
    File(PathBuf),
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone)]
pub enum ComputationOutcome {
    Executed,
    Skipped,
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum FileSetFile {
    /// Input file for an input file, overall execution group outcome for a computation (serialized
    /// bincode for ComputationOutcome).
    MainFile,
    /// Metadata about how the fileset was obtained.
    Metadata,
    /// Any auxiliary file that can be attached to the main input file. For now only used for
    /// outputs of computations.
    /// The first element of the tuple identifies the execution within an execution group.
    AuxiliaryFile(String, ExecutionFile),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum FileSetKind {
    Computation,
    InputFile,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileReadingOutcome {
    /// The file has been deleted, for example because the worker responsible for the execution has
    /// disappeared, or never existed.
    Dropped,
    /// The file has been fully read.
    EndOfFile,
    /// A new chunk of data is available.
    Data(Vec<u8>),
}

impl FileReadingOutcome {
    fn from_data(data: &[u8], offset: usize, chunk_size: usize) -> FileReadingOutcome {
        if offset >= data.len() {
            return FileReadingOutcome::EndOfFile;
        }
        FileReadingOutcome::Data(data[offset..(offset + chunk_size).min(data.len())].to_vec())
    }
}

#[derive(Debug)]
struct FileReadWaiter {
    sender: Option<Sender<FileReadingOutcome>>,
    offset: usize,
    chunk_size: usize,
}

impl FileReadWaiter {
    fn send(&mut self, outcome: FileReadingOutcome) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(outcome);
        }
    }
}

impl Drop for FileReadWaiter {
    fn drop(&mut self) {
        self.send(FileReadingOutcome::Dropped);
    }
}

#[derive(Debug, Default)]
struct FileInfo {
    data: Vec<u8>,
    /// Readers that are waiting for more data to be written.
    readers: Vec<FileReadWaiter>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FileSetStatus {
    /// The file set was not created yet, but at least one reader is waiting for it.
    Pending,
    /// The file set was created, but writing has not started yet.
    Created,
    /// Writing has started but not yet finished.
    Writing,
    /// Writing is complete.
    Finalized,
}

#[derive(Debug)]
pub struct FileSet {
    kind: FileSetKind,
    status: FileSetStatus,
    writing_waiters: Vec<Sender<()>>,
    creation_waiters: Vec<Sender<()>>,
    finalization_waiters: Vec<Sender<()>>,
    files: HashMap<FileSetFile, FileInfo>,
}

fn maybe_wait(
    cond: bool,
    waiters: &mut Vec<Sender<()>>,
) -> impl Future<Output = Result<(), RecvError>> {
    let receiver = if cond {
        let (sender, receiver) = channel();
        waiters.push(sender);
        Some(receiver)
    } else {
        None
    };
    async {
        if let Some(recv) = receiver {
            recv.await
        } else {
            Ok(())
        }
    }
}

impl FileSet {
    pub fn new() -> FileSet {
        FileSet {
            kind: FileSetKind::InputFile,
            status: FileSetStatus::Pending,
            writing_waiters: vec![],
            creation_waiters: vec![],
            finalization_waiters: vec![],
            files: HashMap::new(),
        }
    }

    pub fn create(&mut self, kind: FileSetKind) -> Result<bool, ()> {
        if self.status > FileSetStatus::Pending {
            if self.kind != kind {
                return Err(());
            }
            return Ok(false);
        }
        self.kind = kind;
        self.status = FileSetStatus::Created;
        self.creation_waiters.drain(..).for_each(|waiter| {
            let _ = waiter.send(());
        });
        Ok(true)
    }

    pub fn start_writing(&mut self) -> bool {
        if self.status > FileSetStatus::Created {
            return false;
        }
        self.status = FileSetStatus::Writing;
        self.writing_waiters.drain(..).for_each(|waiter| {
            let _ = waiter.send(());
        });
        true
    }

    pub fn mark_finalized(&mut self) -> bool {
        if self.status > FileSetStatus::Writing {
            return false;
        }
        self.status = FileSetStatus::Finalized;
        self.finalization_waiters.drain(..).for_each(|waiter| {
            let _ = waiter.send(());
        });
        for file in self.files.iter_mut() {
            file.1.readers.drain(..).for_each(|mut waiter| {
                let _ = waiter.send(FileReadingOutcome::from_data(
                    &file.1.data,
                    waiter.offset,
                    waiter.chunk_size,
                ));
            });
        }
        true
    }

    pub fn append_to_file(&mut self, file: &FileSetFile, data: &[u8]) {
        let file_data = self.files.entry(file.clone()).or_default();
        file_data.data.extend_from_slice(data);
        file_data.readers.drain(..).for_each(|mut waiter| {
            let _ = waiter.send(FileReadingOutcome::from_data(
                &file_data.data,
                waiter.offset,
                waiter.chunk_size,
            ));
        });
    }

    pub fn read_from_file(
        &mut self,
        file: &FileSetFile,
        offset: usize,
        chunk_size: usize,
    ) -> impl Future<Output = FileReadingOutcome> {
        let (outcome, receiver) = if !self.files.contains_key(file)
            && self.status == FileSetStatus::Finalized
        {
            (Some(FileReadingOutcome::Dropped), None)
        } else {
            let file_data = self.files.entry(file.clone()).or_default();
            let outcome = FileReadingOutcome::from_data(&file_data.data, offset, chunk_size);
            if outcome != FileReadingOutcome::EndOfFile || self.status == FileSetStatus::Finalized {
                (Some(outcome), None)
            } else {
                let (sender, receiver) = channel();
                file_data.readers.push(FileReadWaiter {
                    sender: Some(sender),
                    offset,
                    chunk_size,
                });
                (None, Some(receiver))
            }
        };
        async {
            if let Some(outcome) = outcome {
                outcome
            } else {
                // The sender for this receiver can never be dropped without having received a
                // message first.
                receiver.unwrap().await.unwrap()
            }
        }
    }

    pub fn wait_for_creation(&mut self) -> impl Future<Output = Result<(), RecvError>> {
        maybe_wait(
            self.status <= FileSetStatus::Pending,
            &mut self.creation_waiters,
        )
    }

    pub fn wait_for_finalization(&mut self) -> impl Future<Output = Result<(), RecvError>> {
        maybe_wait(
            self.status <= FileSetStatus::Writing,
            &mut self.finalization_waiters,
        )
    }

    pub fn wait_for_writable(&mut self) -> impl Future<Output = Result<(), RecvError>> {
        maybe_wait(
            self.status <= FileSetStatus::Created,
            &mut self.writing_waiters,
        )
    }

    pub fn is_finalized(&self) -> bool {
        self.status == FileSetStatus::Finalized
    }

    pub fn kind(&self) -> FileSetKind {
        self.kind
    }
}
