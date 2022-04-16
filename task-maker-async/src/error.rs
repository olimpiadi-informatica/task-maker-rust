use crate::{
    file_set::FileSetFile,
    store::{DataIdentificationHash, FileSetHandleId, FileSetHash},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Error {
    #[error("Computation already exists: {0:?}")]
    ComputationExists(FileSetHash),
    #[error("Invalid hash {0:?}: input file hash used as computation hash or viceversa")]
    HashCollision(FileSetHash),
    #[error("Invalid hash {0:?}: hash does not exist")]
    UnknownHash(FileSetHash),
    #[error("Invalid handle {0}. It may have already been finalized")]
    UnknownHandle(FileSetHandleId),
    #[error("Handle {0} is not active. Call activate_for_writing first.")]
    NotActive(FileSetHandleId),
    #[error("Fileset {0:?} has been dropped or never existed.")]
    FileSetDropped(FileSetHash),
    #[error("{0:?} is not a valid file type for an input file.")]
    InvalidFileForInput(FileSetFile),
    #[error("Invalid hash {1:?} for input file {0:?}.")]
    InvalidHash(DataIdentificationHash, DataIdentificationHash),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}
