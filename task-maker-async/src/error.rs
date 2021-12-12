use crate::store::ComputationHash;
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
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}
