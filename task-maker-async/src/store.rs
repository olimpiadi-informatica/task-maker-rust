#![allow(dead_code, unused_variables)]

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::select;
use tokio::time::interval;

use tokio_util::sync::CancellationToken;

use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::{ClientMessage, Response, Transport};

type HashData = Vec<u8>;

const LEASE_LENGTH: Duration = Duration::from_secs(2);

const CHUNK_SIZE: usize = 1 << 12;

#[derive(Debug, Serialize, Deserialize)]
pub struct InputFileHash(HashData);

/// Two-level hash; the outer hash has information about all properties of a computation that are
/// expected to change the result (such as outer hashes of inputs, or the command line). The inner
/// hash takes care of properties of the computation that should not change the outputs, such as
/// time and memory limits; it also includes the first hash.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ComputationHash(HashData, HashData);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
enum HandleMode {
    Read,
    Write,
}

type FileSetHandleId = usize;
type FileHandleId = usize;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct FileSetHandle {
    id: FileSetHandleId,
    mode: HandleMode,
}

impl FileSetHandle {
    pub fn is_writable(&self) -> bool {
        self.mode == HandleMode::Write
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct FileHandle {
    file_set_handle: FileSetHandle,
    id: FileHandleId,
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone)]
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
    /// Used by the client to upload inputs to the DAG. Returns a writing handle if the input file is
    /// not present, or a reading handle if it is.
    async fn create_or_open_input_file(hash: InputFileHash) -> Result<FileSetHandle, Error>;

    // Creating a computation is not a RPC.

    /// Opens a computed fileset for reading. Creates a lease for the computation data that will prevent it
    /// from being dropped. If the computation is not present, waits until it is created.
    async fn open_computation(hash: ComputationHash) -> Result<FileSetHandle, Error>;

    /// Opens a file inside a fileset. Waits for the file to be created if it doesn't exist yet and
    /// the fileset handle is a reading handle, creates the file otherwise.
    /// Returns an error if the handle is invalid.
    async fn open_file(handle: FileSetHandle, file: FileSetFile) -> Result<FileHandle, Error>;

    /// Appends data to a file in a fileset that is open for writing. Refreshes the writing lease.
    async fn append_chunk(file: FileHandle, data: Vec<u8>) -> Result<(), Error>;

    /// Finalizes a FileSet handle in writing mode. Terminates the writing lease and returns a reading
    /// lease for the same FileSet. If finalizing an input fileset, returns an error if the hash of
    /// its MainFile is not correct.
    async fn finalize_fileset(handle: FileSetHandle) -> Result<FileSetHandle, Error>;

    /// Tries to read from a file. Refreshes the corresponding lease.
    async fn read_chunk(file: FileHandle, offset: usize) -> Result<FileReadingOutcome, Error>;

    /// Refreshes the lease for the given fileset.
    /// It is guaranteed that the fileset will not be deleted while there's an outstanding lease
    /// to it. It is an error to refresh a lease of a non-existent input.
    async fn refresh_fileset_lease(handle: FileSetHandle) -> Result<(), Error>;
}

#[derive(Debug, Eq, PartialEq)]
enum FileSetKind {
    Computation,
    InputFile,
}

type FileContents = Vec<u8>;

#[derive(Debug)]
struct FileSet {
    files: HashMap<FileSetFile, FileContents>,
    kind: FileSetKind,
    finalized: bool,
}

type FileSetVariants = HashMap<HashData, FileSet>;

#[derive(Debug)]
struct FileSetHandleInfo {
    outer_hash: HashData,
    inner_hash: HashData,
    expiration: Instant,
    mode: HandleMode,
    file_handles: HashMap<FileHandleId, FileSetFile>,
    next_handle: FileHandleId,
}

struct StoreServiceImpl {
    filesets: HashMap<HashData, FileSetVariants>,
    file_set_handles: HashMap<FileSetHandleId, FileSetHandleInfo>,
    next_handle: FileSetHandleId,
}

impl StoreServiceImpl {
    fn new() -> Self {
        StoreServiceImpl {
            filesets: HashMap::new(),
            file_set_handles: HashMap::new(),
            next_handle: 0,
        }
    }

    fn new_handle(&mut self, hash: ComputationHash, mode: HandleMode) -> FileSetHandle {
        let handle = self.next_handle;
        self.next_handle += 1;
        let ComputationHash(outer_hash, inner_hash) = hash;
        let handle_info = FileSetHandleInfo {
            outer_hash,
            inner_hash,
            expiration: Instant::now() + LEASE_LENGTH,
            mode,
            file_handles: HashMap::new(),
            next_handle: 0,
        };
        self.file_set_handles.insert(handle, handle_info);
        FileSetHandle { id: handle, mode }
    }

    fn create_or_open_fileset(
        &mut self,
        hash: ComputationHash,
        kind: FileSetKind,
    ) -> Result<FileSetHandle, Error> {
        let outer_comp = self.filesets.entry(hash.0.clone()).or_default();
        if let Some(file_set) = outer_comp.get(&hash.1) {
            if file_set.kind != kind {
                return Err(Error::HashCollision(hash));
            }
            Ok(self.new_handle(hash, HandleMode::Read))
        } else {
            outer_comp.insert(
                hash.1.clone(),
                FileSet {
                    files: HashMap::new(),
                    kind,
                    finalized: false,
                },
            );
            Ok(self.new_handle(hash, HandleMode::Write))
        }
    }

    fn clear_expired_leases(&mut self) {
        let time = Instant::now();
        for info in self.file_set_handles.values() {
            if info.expiration < time && info.mode == HandleMode::Write {
                self.filesets
                    .get_mut(&info.outer_hash)
                    .unwrap()
                    .remove(&info.inner_hash);
            }
        }
        self.file_set_handles
            .retain(|_, info| info.expiration >= time);
    }

    fn validate_and_refresh(
        &mut self,
        handle: FileSetHandle,
    ) -> Result<Entry<'_, FileSetHandleId, FileSetHandleInfo>, Error> {
        let mut entry = self.file_set_handles.entry(handle.id);
        match entry {
            Entry::Vacant(_) => return Err(Error::UnknownHandle(handle.id)),
            Entry::Occupied(ref mut entry) => {
                let mut v = entry.get_mut();
                if v.mode != handle.mode {
                    return Err(Error::UnknownHandle(handle.id));
                } else {
                    v.expiration = Instant::now() + LEASE_LENGTH;
                }
            }
        }
        Ok(entry)
    }

    fn append_to_file(&mut self, file: FileHandle, mut data: Vec<u8>) {
        let fileset_info = self.file_set_handles.get(&file.file_set_handle.id).unwrap();
        let file_info = fileset_info.file_handles.get(&file.id).unwrap();
        // TODO(veluca): update hash if appending to an input file.
        self.filesets
            .get_mut(&fileset_info.outer_hash)
            .unwrap()
            .get_mut(&fileset_info.inner_hash)
            .unwrap()
            .files
            .get_mut(file_info)
            .unwrap()
            .append(&mut data);
    }

    fn open_file(
        &mut self,
        file_set_handle: FileSetHandle,
        file: FileSetFile,
    ) -> Result<FileHandle, Error> {
        let fileset_info = self.file_set_handles.get_mut(&file_set_handle.id).unwrap();
        let fileset_group = self.filesets.get_mut(&fileset_info.outer_hash);
        if fileset_group.is_none() {
            return Err(Error::FileSetDropped(file_set_handle.id));
        }
        let fileset = fileset_group.unwrap().get_mut(&fileset_info.inner_hash);
        if fileset.is_none() {
            return Err(Error::FileSetDropped(file_set_handle.id));
        }
        let fileset = fileset.unwrap();
        if !fileset.finalized && file_set_handle.mode == HandleMode::Read {
            return Err(Error::NotImplemented(
                "Opening a file for reading in a non-finalized fileset is not implemented".into(),
            ));
        }
        // TODO(veluca): error out if trying to obtain a second writing handle to the same file.
        if !fileset.files.contains_key(&file) {
            if file_set_handle.mode == HandleMode::Read {
                return Err(Error::NonExistentFile(file, file_set_handle.id));
            } else if fileset.kind == FileSetKind::InputFile
                && matches!(file, FileSetFile::AuxiliaryFile(_))
            {
                return Err(Error::InvalidFileForInput(file));
            } else {
                fileset.files.insert(file.clone(), vec![]);
            }
        }
        let file_handle = fileset_info.next_handle;
        fileset_info.next_handle += 1;
        fileset_info.file_handles.insert(file_handle, file);
        Ok(FileHandle {
            file_set_handle,
            id: file_handle,
        })
    }

    fn read_from_file(
        &mut self,
        file: FileHandle,
        offset: usize,
    ) -> Result<FileReadingOutcome, Error> {
        let fileset_info = self.file_set_handles.get(&file.file_set_handle.id).unwrap();
        let file_info = fileset_info.file_handles.get(&file.id).unwrap();
        let fileset_group = self.filesets.get(&fileset_info.outer_hash);
        if fileset_group.is_none() {
            return Ok(FileReadingOutcome::Dropped);
        }
        let fileset = fileset_group.unwrap().get(&fileset_info.inner_hash);
        if fileset.is_none() {
            return Ok(FileReadingOutcome::Dropped);
        }
        let fileset = fileset.unwrap();
        if !fileset.finalized {
            return Err(Error::NotImplemented(
                "Reading from a non-finalized fileset is not implemented".into(),
            ));
        }
        let file = fileset.files.get(file_info).unwrap();
        if file.len() <= offset {
            return Ok(FileReadingOutcome::EndOfFile);
        }
        let end = (offset + CHUNK_SIZE).min(file.len());
        Ok(FileReadingOutcome::Data(file[offset..end].to_vec()))
    }
}

/// In-memory implementation of the Store interface.
/// TODO(veluca): write a proper disk-backed implementation.
#[derive(Clone)]
pub struct StoreService {
    service: Arc<Mutex<StoreServiceImpl>>,
    cancellation_token: Arc<CancellationToken>,
}

impl StoreService {
    /// Creates the storage for a given computation; this method is called by the server to obtain
    /// a writing handle that the workers can use. It is an error to create a computation if
    /// another computation with the same hash already exists, even if it is temporary (i.e. not
    /// finalized).
    pub fn create_computation(&self, hash: ComputationHash) -> Result<FileSetHandle, Error> {
        let mut service = self.service.lock().unwrap();
        let handle = service.create_or_open_fileset(hash.clone(), FileSetKind::Computation)?;
        if handle.is_writable() {
            Ok(handle)
        } else {
            Err(Error::ComputationExists(hash))
        }
    }

    /// Lists all the finalized computations that are similar to the given computation, i.e. for
    /// which the first part of the hash matches.
    pub fn similar_computations(&self, hash: ComputationHash) -> Vec<ComputationHash> {
        let service = self.service.lock().unwrap();
        let hash_top = hash.0;
        match service.filesets.get(&hash_top) {
            Some(variants) => variants
                .iter()
                .filter_map(|(k, v)| if v.finalized { Some(k) } else { None })
                .map(|x| ComputationHash(hash_top.clone(), x.clone()))
                .collect(),
            None => vec![],
        }
    }

    /// Creates and starts a store, listening for RPCs on the given transport.
    pub fn new_on_transport(
        transport: impl Transport<Response<StoreResponse>, ClientMessage<StoreRequest>> + Send + 'static,
    ) -> Self {
        let token = CancellationToken::new();

        let service = StoreService {
            service: Arc::new(Mutex::new(StoreServiceImpl::new())),
            cancellation_token: Arc::new(token),
        };
        let server = BaseChannel::with_defaults(transport);

        {
            let service = service.clone();
            let token = service.cancellation_token.clone();
            tokio::spawn(async move {
                select! {
                    _ = token.cancelled() => {}
                    _ = server.execute(service.serve()) => {}
                }
            });
        }

        {
            let token = service.cancellation_token.clone();
            let service = service.clone();
            tokio::spawn(async move {
                let mut timer = interval(LEASE_LENGTH);
                let timer = async {
                    loop {
                        let x = timer.tick().await;
                        {
                            service.service.lock().unwrap().clear_expired_leases();
                        }
                    }
                };

                select! {
                    _ = token.cancelled() => {}
                    _ = timer => {}
                }
            });
        }
        service
    }

    /// Stops the server.
    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }
}

#[tarpc::server]
impl Store for StoreService {
    async fn create_or_open_input_file(
        self,
        context: Context,
        hash: InputFileHash,
    ) -> Result<FileSetHandle, Error> {
        let mut service = self.service.lock().unwrap();
        service.create_or_open_fileset(
            ComputationHash(hash.0.clone(), hash.0),
            FileSetKind::InputFile,
        )
    }

    async fn open_computation(
        self,
        context: Context,
        hash: ComputationHash,
    ) -> Result<FileSetHandle, Error> {
        let has_comp = {
            let service = self.service.lock().unwrap();
            service
                .filesets
                .get(&hash.0)
                .and_then(|x| x.get(&hash.1))
                .is_some()
        };

        if !has_comp {
            return Err(Error::NotImplemented(
                "Waiting for computation creation is not yet implemented".into(),
            ));
        }

        let mut service = self.service.lock().unwrap();
        let file_set = service
            .filesets
            .get(&hash.0)
            .and_then(|x| x.get(&hash.1))
            .unwrap();
        if file_set.kind != FileSetKind::Computation {
            return Err(Error::HashCollision(hash));
        }
        Ok(service.new_handle(hash, HandleMode::Read))
    }

    async fn finalize_fileset(
        self,
        context: Context,
        handle: FileSetHandle,
    ) -> Result<FileSetHandle, Error> {
        if handle.mode == HandleMode::Read {
            return Err(Error::FinalizeRead(handle.id));
        }
        let mut service = self.service.lock().unwrap();
        let service = &mut *service;
        // TODO(veluca): we should verify the hash of the file if this is a fileset.
        {
            let entry = service.validate_and_refresh(handle)?;
            entry
                .and_modify(|v| v.mode = HandleMode::Read)
                .and_modify(|v| v.file_handles.clear());
        }

        let handle_info = service.file_set_handles.get(&handle.id).unwrap();
        service
            .filesets
            .get_mut(&handle_info.outer_hash)
            .unwrap()
            .get_mut(&handle_info.inner_hash)
            .unwrap()
            .finalized = true;

        Ok(FileSetHandle {
            id: handle.id,
            mode: HandleMode::Read,
        })
    }

    async fn refresh_fileset_lease(
        self,
        context: Context,
        handle: FileSetHandle,
    ) -> Result<(), Error> {
        let mut service = self.service.lock().unwrap();
        service.validate_and_refresh(handle).map(|x| ())
    }

    async fn open_file(
        self,
        context: Context,
        handle: FileSetHandle,
        file: FileSetFile,
    ) -> Result<FileHandle, Error> {
        let mut service = self.service.lock().unwrap();
        service.validate_and_refresh(handle)?;
        service.open_file(handle, file)
    }

    async fn append_chunk(
        self,
        context: Context,
        file: FileHandle,
        data: Vec<u8>,
    ) -> Result<(), Error> {
        let mut service = self.service.lock().unwrap();
        service.validate_and_refresh(file.file_set_handle)?;
        if file.file_set_handle.mode != HandleMode::Write {
            return Err(Error::AppendRead(file.file_set_handle.id, file.id));
        }
        service.append_to_file(file, data);
        Ok(())
    }

    async fn read_chunk(
        self,
        context: Context,
        file: FileHandle,
        offset: usize,
    ) -> Result<FileReadingOutcome, Error> {
        let mut service = self.service.lock().unwrap();
        service.validate_and_refresh(file.file_set_handle)?;
        if file.file_set_handle.mode != HandleMode::Read {
            return Err(Error::ReadWrite(file.file_set_handle.id, file.id));
        }
        service.read_from_file(file, offset)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert2::{check, let_assert};
    use tarpc::context;

    #[tokio::test]
    async fn fileset_basic() {
        let (client_transport, server_transport) = tarpc::transport::channel::unbounded();
        let client = StoreClient::new(tarpc::client::Config::default(), client_transport).spawn();
        let context = context::current();
        let server = StoreService::new_on_transport(server_transport);

        // Obtain a writing lease.
        let resp = client
            .create_or_open_input_file(context, InputFileHash(vec![]))
            .await
            .unwrap();
        let_assert!(Ok(handle1) = resp);
        check!(handle1.is_writable());

        // Obtain a reading lease.
        let resp = client
            .create_or_open_input_file(context, InputFileHash(vec![]))
            .await
            .unwrap();
        let_assert!(Ok(handle2) = resp);
        check!(!handle2.is_writable());

        // Refresh the writing lease.
        let resp = client
            .refresh_fileset_lease(context::current(), handle1)
            .await
            .unwrap();
        check!(Ok(()) == resp);

        // Finalize the writing lease.
        let resp = client
            .finalize_fileset(context::current(), handle1)
            .await
            .unwrap();
        let_assert!(Ok(handle1) = resp);
        check!(!handle1.is_writable());

        // Refresh the resulting reading lease.
        let resp = client
            .refresh_fileset_lease(context::current(), handle1)
            .await
            .unwrap();
        check!(Ok(()) == resp);

        // Wait long enough for the reading lease to expire.
        tokio::time::sleep(LEASE_LENGTH + Duration::from_millis(100)).await;

        let resp = client
            .refresh_fileset_lease(context::current(), handle2)
            .await
            .unwrap();

        let_assert!(Err(_) = resp);
    }

    #[tokio::test]
    async fn write_read_file() {
        let (client_transport, server_transport) = tarpc::transport::channel::unbounded();
        let client = StoreClient::new(tarpc::client::Config::default(), client_transport).spawn();
        let context = context::current();
        let server = StoreService::new_on_transport(server_transport);

        // Obtain a fileset lease.
        let resp = client
            .create_or_open_input_file(context, InputFileHash(vec![]))
            .await
            .unwrap();
        let_assert!(Ok(handle1) = resp);
        check!(handle1.is_writable());

        // Obtain a file lease.
        let resp = client
            .open_file(context, handle1, FileSetFile::MainFile)
            .await
            .unwrap();
        let_assert!(Ok(handle2) = resp);

        // Append data to the file.
        client
            .append_chunk(context, handle2, vec![0u8; CHUNK_SIZE])
            .await
            .unwrap()
            .unwrap();

        // Append one more chunk to the file.
        client
            .append_chunk(context, handle2, vec![1u8; CHUNK_SIZE])
            .await
            .unwrap()
            .unwrap();

        // Finalize the fileset.
        let handle1 = client
            .finalize_fileset(context, handle1)
            .await
            .unwrap()
            .unwrap();

        // Obtain a reading file lease.
        let resp = client
            .open_file(context, handle1, FileSetFile::MainFile)
            .await
            .unwrap();
        let_assert!(Ok(handle2) = resp);

        let mut data = vec![];
        loop {
            let outcome = client
                .read_chunk(context, handle2, data.len())
                .await
                .unwrap()
                .unwrap();
            match outcome {
                FileReadingOutcome::Dropped => {
                    unreachable!("invalid outcome");
                }
                FileReadingOutcome::EndOfFile => {
                    break;
                }
                FileReadingOutcome::Data(mut chunk) => {
                    data.append(&mut chunk);
                }
            }
        }

        assert!(data[0..CHUNK_SIZE] == [0u8; CHUNK_SIZE]);
        assert!(data[CHUNK_SIZE..2 * CHUNK_SIZE] == [1u8; CHUNK_SIZE]);
    }
}
