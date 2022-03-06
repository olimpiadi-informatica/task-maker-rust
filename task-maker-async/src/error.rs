use crate::store::{
    DataIdentificationHash, FileHandleId, FileSetFile, FileSetHandleId, VariantIdentificationHash,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Error {
    #[error("Computation already exists: {0:?} variant {1:?}")]
    ComputationExists(DataIdentificationHash, VariantIdentificationHash),
    #[error("Invalid hash {0:?}: input file hash used as computation hash or viceversa")]
    HashCollision(DataIdentificationHash),
    #[error("Invalid handle {0}. It may have expired or have the wrong mode.")]
    UnknownHandle(FileSetHandleId),
    #[error("Trying to finalize a read-only handle {0}.")]
    FinalizeRead(FileSetHandleId),
    #[error("Trying to append to read-only handle {0}:{1}.")]
    AppendRead(FileSetHandleId, FileHandleId),
    #[error("Trying to read from a write-only handle {0}:{1}.")]
    ReadWrite(FileSetHandleId, FileHandleId),
    #[error("Fileset has been dropped {0}.")]
    FileSetDropped(FileSetHandleId),
    #[error("File {0:?} does not exist in fileset {1}.")]
    NonExistentFile(FileSetFile, FileSetHandleId),
    #[error("{0:?} is not a valid file type for an input file.")]
    InvalidFileForInput(FileSetFile),
    #[error("Invalid hash {1:?} for input file {0:?}.")]
    InvalidHash(DataIdentificationHash, DataIdentificationHash),
    #[error("{0:?} is already open for writing in fileset {1}.")]
    MultipleWrites(FileSetFile, FileSetHandleId),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}
