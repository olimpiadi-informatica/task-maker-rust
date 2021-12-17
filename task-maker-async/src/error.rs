use crate::store::{ComputationHash, FileSetFile};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Error {
    #[error("Computation already exists: {0:?}")]
    ComputationExists(ComputationHash),
    #[error("Invalid hash {0:?}: input file hash used as computation hash or viceversa")]
    HashCollision(ComputationHash),
    #[error("Invalid handle {0}. It may have expired or have the wrong mode.")]
    UnknownHandle(usize),
    #[error("Trying to finalize a read-only handle {0}.")]
    FinalizeRead(usize),
    #[error("Trying to append to read-only handle {0}:{1}.")]
    AppendRead(usize, usize),
    #[error("Trying to read from a write-only handle {0}:{1}.")]
    ReadWrite(usize, usize),
    #[error("Fileset has been dropped {0}.")]
    FileSetDropped(usize),
    #[error("File {0:?} does not exist in fileset {1}.")]
    NonExistentFile(FileSetFile, usize),
    #[error("{0:?} is not a valid file type for an input file.")]
    InvalidFileForInput(FileSetFile),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}
