#![allow(dead_code, unused_variables)]

use std::collections::{hash_map::Entry, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::{ClientMessage, Response, Transport};
use tokio::select;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::error::Error;

type HashData = Vec<u8>;

const LEASE_LENGTH: Duration = Duration::from_secs(2);

const CHUNK_SIZE: usize = 1 << 12;

#[derive(Debug, Serialize, Deserialize)]
pub struct InputFileHash(HashData);

/// Two-level hash; the outer hash has information about all properties of a computation that are
/// expected to change the result (such as outer hashes of inputs, or the command line). The inner
/// hash takes care of properties of the computation that should not change the outputs, such as
/// time and memory limits; it also includes the first hash.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
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
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use assert2::{assert, check, let_assert};
    use tarpc::client::RpcError;
    use tarpc::context;

    use super::*;

    fn spawn() -> (StoreClient, Context, StoreService) {
        let (client_transport, server_transport) = tarpc::transport::channel::unbounded();
        let client = StoreClient::new(tarpc::client::Config::default(), client_transport).spawn();
        let context = context::current();
        let server = StoreService::new_on_transport(server_transport);
        (client, context, server)
    }

    fn get_hash<T: Hash>(data: T) -> HashData {
        // TODO: this is just a mock of the hash
        let mut hasher = DefaultHasher::default();
        data.hash(&mut hasher);
        hasher.finish().to_le_bytes().into()
    }

    #[tokio::test]
    async fn test_write_and_read_files() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();

        let hash = get_hash("input1");
        let resp = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?;
        let_assert!(Ok(fileset_handle) = resp);
        check!(fileset_handle.is_writable());

        for (i, file_type) in [(0, FileSetFile::MainFile), (1, FileSetFile::Metadata)] {
            let resp = client.open_file(context, fileset_handle, file_type).await?;
            let_assert!(Ok(file_handle) = resp);

            let resp = client
                .append_chunk(context, file_handle, vec![i, i, i])
                .await?;
            let_assert!(Ok(()) = resp);

            let resp = client
                .append_chunk(context, file_handle, vec![42, 42])
                .await?;
            let_assert!(Ok(()) = resp);
        }

        let resp = client.finalize_fileset(context, fileset_handle).await?;
        let_assert!(Ok(fileset_handle) = resp);
        check!(!fileset_handle.is_writable());

        let resp = client
            .create_or_open_input_file(context, InputFileHash(hash))
            .await?;
        let_assert!(Ok(fileset_handle2) = resp);
        check!(!fileset_handle2.is_writable());

        let resp = client
            .open_file(context, fileset_handle, FileSetFile::MainFile)
            .await?;
        let_assert!(Ok(file_handle1) = resp);

        let resp = client
            .open_file(context, fileset_handle2, FileSetFile::MainFile)
            .await?;
        let_assert!(Ok(file_handle2) = resp);

        let resp = client.read_chunk(context, file_handle1, 0).await?;
        let_assert!(Ok(FileReadingOutcome::Data(data1)) = resp);
        assert!(data1 == vec![0, 0, 0, 42, 42]);

        let resp = client.read_chunk(context, file_handle2, 0).await?;
        let_assert!(Ok(FileReadingOutcome::Data(data2)) = resp);
        assert!(data2 == vec![0, 0, 0, 42, 42]);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_not_existent() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let comp_hash = ComputationHash(hash.clone(), hash);
        let write_handle = server.create_computation(comp_hash.clone()).unwrap();
        let file = client
            .open_file(
                context,
                write_handle,
                FileSetFile::AuxiliaryFile("file".into()),
            )
            .await?
            .unwrap();
        client
            .finalize_fileset(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client.open_computation(context, comp_hash).await?.unwrap();
        check!(!read_handle.is_writable());

        let file = FileSetFile::AuxiliaryFile("lolnope".into());
        let resp = client.open_file(context, read_handle, file.clone()).await?;
        let_assert!(Err(Error::NonExistentFile(file, _)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_write_with_readonly() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let file = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .append_chunk(context, file, vec![1, 2, 3])
            .await?
            .unwrap();
        client
            .finalize_fileset(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        check!(!read_handle.is_writable());
        let file = client
            .open_file(context, read_handle, FileSetFile::MainFile)
            .await?
            .unwrap();

        let resp = client.append_chunk(context, file, vec![1, 2, 3]).await?;
        let_assert!(Err(Error::AppendRead(_, _)) = resp);

        let resp = client.read_chunk(context, file, 0).await?;
        let_assert!(Ok(FileReadingOutcome::Data(_)) = resp);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_with_writeonly() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let file = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        let resp = client.read_chunk(context, file, 0).await?;
        let_assert!(Err(Error::ReadWrite(_, _)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_finalize_with_readonly() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        client
            .finalize_fileset(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        check!(!read_handle.is_writable());
        let resp = client.finalize_fileset(context, read_handle).await?;
        let_assert!(Err(Error::FinalizeRead(_)) = resp);
        Ok(())
    }

    #[tokio::test]
    async fn test_no_auxiliary_file_for_input() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash))
            .await?
            .unwrap();
        let file = FileSetFile::AuxiliaryFile("file".into());
        let resp = client
            .open_file(context, write_handle, file.clone())
            .await?;
        let_assert!(Err(Error::InvalidFileForInput(file)) = resp);
        Ok(())
    }

    #[tokio::test]
    async fn test_chunked_read() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let file = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        let mut expected = vec![];
        // 5 full chunks
        for i in 0..5 {
            let mut chunk = vec![i; CHUNK_SIZE];
            client
                .append_chunk(context, file, chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        // 5 small chunks
        for i in 0..5 {
            let mut chunk = vec![i; 3];
            client
                .append_chunk(context, file, chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        // 5 full chunks
        for i in 0..5 {
            let mut chunk = vec![i; CHUNK_SIZE];
            client
                .append_chunk(context, file, chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        client
            .finalize_fileset(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        check!(!read_handle.is_writable());
        let file = client
            .open_file(context, read_handle, FileSetFile::MainFile)
            .await?
            .unwrap();

        let mut data = vec![];
        loop {
            let outcome = client.read_chunk(context, file, data.len()).await?.unwrap();
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
        assert!(data == expected);
        Ok(())
    }

    #[tokio::test]
    #[ignore] // because opening a file for reading when not finalized is not implemented yet
    async fn test_read_dropped() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash))
            .await?
            .unwrap();

        let file_w = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        let file_r = client
            .open_file(context, read_handle, FileSetFile::MainFile)
            .await?
            .unwrap();

        client
            .append_chunk(context, file_w, vec![1, 2, 3])
            .await?
            .unwrap();

        client.read_chunk(context, file_r, 0).await?.unwrap();

        // drop write_handle but keep read_handle alive
        for _ in 0..4 {
            tokio::time::sleep(LEASE_LENGTH / 2).await;
            client
                .refresh_fileset_lease(context, read_handle)
                .await?
                .unwrap();
        }
        // 2 * LEASE_LENGTH has passed, the write handle for sure is gone
        let resp = client.read_chunk(context, file_r, 3).await?;
        let_assert!(Ok(FileReadingOutcome::Dropped) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_open_write_lease_expired() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_open_read_lease_expired() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client
            .open_file(context, read_handle, FileSetFile::MainFile)
            .await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_append_lease_expired() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let file_w = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .append_chunk(context, file_w, vec![1, 2, 3])
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client.append_chunk(context, file_w, vec![4, 5, 6]).await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_read_lease_expired() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .finalize_fileset(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let file = client
            .open_file(context, read_handle, FileSetFile::MainFile)
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client.read_chunk(context, file, 0).await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_lease_expired() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client.refresh_fileset_lease(context, write_handle).await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let input_file_handle = client
            .create_or_open_input_file(context, InputFileHash(hash.clone()))
            .await?
            .unwrap();
        let resp = client
            .open_computation(context, ComputationHash(hash.clone(), hash))
            .await?;
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision2() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        server
            .create_computation(ComputationHash(hash.clone(), hash.clone()))
            .unwrap();
        let resp = client
            .create_or_open_input_file(context, InputFileHash(hash))
            .await?;
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_computation_exists() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let comp_hash = ComputationHash(hash.clone(), hash);
        server.create_computation(comp_hash.clone()).unwrap();
        let resp = server.create_computation(comp_hash);
        let_assert!(Err(Error::ComputationExists(hash)) = resp);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // because waiting for computation creation is not implemented yet
    async fn test_wait_computation_creation() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let comp_hash = ComputationHash(hash.clone(), hash);

        let mut fut = Box::pin(client.open_computation(context, comp_hash.clone()));
        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            resp = &mut fut => panic!("should be waiting for the computation, but got: {:?}", resp),
        }

        let comp = server.create_computation(comp_hash).unwrap();

        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => panic!("the computation should be ready now"),
            resp = fut => assert!(let Ok(_) = resp),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_similar_computations() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let outer1 = get_hash("outer1");
        let outer2 = get_hash("outer2");
        let inner1 = get_hash("inner1");
        let inner2 = get_hash("inner2");
        for outer in [&outer1, &outer2] {
            for inner in [&inner1, &inner2] {
                let handle = server
                    .create_computation(ComputationHash(outer.clone(), inner.clone()))
                    .unwrap();
                client
                    .finalize_fileset(context, handle)
                    .await
                    .unwrap()
                    .unwrap();
            }
        }

        let mut similar =
            server.similar_computations(ComputationHash(outer1.clone(), inner1.clone()));
        let mut expected = vec![
            ComputationHash(outer1.clone(), inner1.clone()),
            ComputationHash(outer1.clone(), inner2.clone()),
        ];
        similar.sort();
        expected.sort();
        assert!(similar == expected);

        let similar =
            server.similar_computations(ComputationHash(get_hash("lolnope"), get_hash("nope")));
        assert!(similar.is_empty());

        Ok(())
    }
}
