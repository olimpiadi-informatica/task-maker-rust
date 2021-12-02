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
pub struct InputFileWriteHandle(usize);
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputationWriteHandle(usize);
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputationReadHandle(usize);
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadHandle(usize);

#[derive(Debug, Serialize, Deserialize)]
pub enum ComputationDataFile {
    Outcome,
    OutputFile(String),
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
    /// Used by the client to upload inputs to the DAG. Returns None if the file is already
    /// present. Either way, this leases the corresponding hash for LEASE_LENGTH_SEC.
    async fn create_input_file(hash: InputFileHash) -> Result<InputFileWriteHandle, ()>;

    /// Appends a chunk to the input file; the InputFileWriteHandle must have been obtained through
    /// create_input_file. Renews the lease on the corresponding hash.
    async fn append_input_chunk(file_handle: InputFileWriteHandle, data: Vec<u8>)
        -> Result<(), ()>;

    /// Finalizes the input file; no more data can be appended, and the resulting hash must match
    /// the claimed hash when create_input_file was called. Returns a read handle to the finalized
    /// file.
    async fn finalize_input_write_file(file_handle: InputFileWriteHandle)
        -> Result<ReadHandle, ()>;

    /// Refreshes the lease for the given input file.
    /// It is guaranteed that the input file will not be deleted while there's an outstanding lease
    /// to it. It is an error to refresh a lease of a non-existent input file.
    async fn refresh_input_lease(file_handle: InputFileWriteHandle) -> Result<(), ()>;

    /// Refreshes the given computation writing lease. Used by workers while they're running an
    /// evaluation group.
    async fn refresh_computation_write_lease(handle: ComputationWriteHandle) -> Result<(), ()>;

    /// Appends data to a file in the output of a computation. The file is created if it does not
    /// exist. Refreshes the writing lease.
    async fn append_computation_chunk(
        handle: ComputationWriteHandle,
        file: ComputationDataFile,
        data: Vec<u8>,
    ) -> Result<(), ()>;

    /// Finalizes a computation output. Terminates the writing lease.
    async fn finalize_computation(handle: ComputationWriteHandle);

    /// Opens an input file for reading. Creates a lease for the input file that will prevent it
    /// from being dropped. If the file is not present, returns an error immediately.
    async fn read_input_file(hash: InputFileHash) -> Result<ReadHandle, ()>;

    /// Opens a computed file for reading. Creates a lease for the computation data that will prevent it
    /// from being dropped. If the computation is not present, waits until it is created.
    async fn open_computation(hash: ComputationHash) -> Result<ComputationReadHandle, ()>;

    /// Opens a computed file for reading. Creates a lease for the computation data that will prevent it
    /// from being dropped. This lease is shared with the per-computation lease: whenever one
    /// expires, so do the others. If the file is not present, waits until the file is created.
    /// In particular, the server will use this method to read the outcome of the computation; when
    /// it reads a EndOfFile response from read_chunk, it will know that the computation has
    /// terminated.
    async fn read_computation_data_file(
        computation: ComputationReadHandle,
        file: ComputationDataFile,
    ) -> Result<ReadHandle, ()>;

    /// Tries to read from a file. Refreshes the corresponding lease.
    async fn read_chunk(file: ReadHandle, offset: usize) -> FileReadingOutcome;

    /// Refreshes the given reading lease.
    async fn refresh_reading_lease(file: ReadHandle) -> Result<(), ()>;
}

pub struct StoreService {
    // TODO
}

impl StoreService {
    /// Creates the storage for a given computation; this method is called by the server to obtain
    /// a writing handle that the workers can use. It is an error to create a computation if
    /// another computation with the same hash already exists, even if it is temporary (i.e. not
    /// finalized).
    pub fn create_computation(&self, _hash: ComputationHash) -> Result<ComputationWriteHandle, ()> {
        todo!("");
    }

    /// Lists all the computations that are similar to the given computation, i.e. for which the
    /// first part of the hash matches.
    pub fn similar_computations(&self, _hash: ComputationHash) -> Vec<ComputationHash> {
        todo!("");
    }

    /// Deletes a computation. This can be used to signal workers to terminate a computation early.
    pub fn kill_computation(&self, _hash: ComputationHash) -> Result<(), ()> {
        todo!("");
    }
}
