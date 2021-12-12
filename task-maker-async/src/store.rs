#![allow(dead_code)]

use serde::{Deserialize, Serialize};

type HashData = Vec<u8>;

#[derive(Debug, Serialize, Deserialize)]
pub struct InputFileHash(HashData);

/// Two-level hash; the outer hash has information about all properties of a computation that are
/// expected to change the result (such as outer hashes of inputs, or the command line). The inner
/// hash takes care of properties of the computation that should not change the outputs, such as
/// time and memory limits; it also includes the first hash.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputationHash(HashData, HashData);

#[derive(Debug, Serialize, Deserialize)]
pub struct FileSetHandle(usize);
#[derive(Debug, Serialize, Deserialize)]
pub struct FileHandle(usize);

#[derive(Debug, Serialize, Deserialize)]
pub enum FileSetFile {
    /// Outcome for a computation, input file for an input file.
    MainFile,
    /// Metadata about how the fileset was obtained.
    Metadata,
    /// Any auxiliary file that can be attached to the main input file. For now only used for
    /// outputs of computations.
    AuxiliaryFile(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FileReadingOutcome {
    /// The file has been deleted, for example because the worker responsible for the execution has
    /// disappeared.
    Dropped,
    /// The file has been fully read.
    EndOfFile,
    /// A new chunk of data is available.
    Data(Vec<u8>),
}

#[tarpc::service]
pub trait Store {
    /// Used by the client to upload inputs to the DAG. Returns an error if the file set is already
    /// present. Either way, this leases the corresponding hash for LEASE_LENGTH_SEC.
    async fn create_input_fileset(hash: InputFileHash) -> Result<FileSetHandle, ()>;

    /// Opens a file for reading. Creates a lease for the input fileset that will prevent it
    /// from being dropped. If the file is not present and finalized, returns an error immediately.
    async fn open_input_file(hash: InputFileHash) -> Result<FileSetHandle, ()>;

    // Creating a computation is not a RPC.

    /// Opens a computed fileset for reading. Creates a lease for the computation data that will prevent it
    /// from being dropped. If the computation is not present, waits until it is created.
    async fn open_computation(hash: ComputationHash) -> FileSetHandle;

    /// Opens a file inside a fileset. Waits for the file to be created if it doesn't exist yet and
    /// the fileset handle is a reading handle, creates the file otherwise.
    /// Returns an error if the handle is invalid.
    async fn open_file(handle: FileSetHandle, file: FileSetFile) -> Result<FileHandle, ()>;

    /// Appends data to a file in a fileset that is open for writing. Refreshes the writing lease.
    async fn append_chunk(file: FileHandle, data: Vec<u8>) -> Result<(), ()>;

    /// Finalizes a FileSet handle in writing mode. Terminates the writing lease and returns a reading
    /// lease for the same FileSet. If finalizing an input fileset, returns an error if the hash of
    /// its MainFile is not correct.
    async fn finalize_fileset(handle: FileSetHandle) -> Result<FileSetHandle, ()>;

    /// Tries to read from a file. Refreshes the corresponding lease.
    async fn read_chunk(file: FileHandle, offset: usize) -> FileReadingOutcome;

    /// Refreshes the lease for the given fileset.
    /// It is guaranteed that the fileset will not be deleted while there's an outstanding lease
    /// to it. It is an error to refresh a lease of a non-existent input.
    async fn refresh_fileset_lease(handle: FileSetHandle) -> Result<(), ()>;
}

pub struct StoreService {
    // TODO
}

impl StoreService {
    /// Creates the storage for a given computation; this method is called by the server to obtain
    /// a writing handle that the workers can use. It is an error to create a computation if
    /// another computation with the same hash already exists, even if it is temporary (i.e. not
    /// finalized).
    pub fn create_computation(&self, _hash: ComputationHash) -> Result<FileSetHandle, ()> {
        todo!("");
    }

    /// Lists all the computations that are similar to the given computation, i.e. for which the
    /// first part of the hash matches.
    pub fn similar_computations(&self, _hash: ComputationHash) -> Vec<ComputationHash> {
        todo!("");
    }
}
