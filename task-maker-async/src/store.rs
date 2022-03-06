#![allow(dead_code, unused_variables)]

use std::collections::{hash_map::Entry, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::{ClientMessage, Response, Transport};
use tokio::select;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::error::Error;

type HashData = [u8; 32];

const LEASE_LENGTH: Duration = Duration::from_secs(2);

const CHUNK_SIZE: usize = 4 * 1024; // 4 KiB

/// Hash that uniquely identifies the *content* of a given fileset.
pub type DataIdentificationHash = HashData;

/// Hash that uniquely identifies a *variant* of a given fileset.
pub type VariantIdentificationHash = HashData;

/// Two-level hash; the DataIdentificationHash has information about all properties of a fileset
/// that are expected to change the result (such as data hashes of inputs, or the command line).
/// The VariantIdentificationHash takes care of properties of the computation that should not
/// change the outputs, such as time and memory limits; it also includes the first hash.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FileSetHash(DataIdentificationHash, VariantIdentificationHash);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
enum HandleMode {
    Read,
    Write,
}

pub type FileSetHandleId = usize;
pub type FileHandleId = usize;

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
    async fn create_or_open_input_file(
        hash: DataIdentificationHash,
    ) -> Result<FileSetHandle, Error>;

    // Creating a computation is not a RPC.

    /// Opens a computed fileset for reading. Creates a lease for the computation data that will prevent it
    /// from being dropped. If the computation is not present, waits until it is created.
    async fn open_computation(
        computation: DataIdentificationHash,
        variant: VariantIdentificationHash,
    ) -> Result<FileSetHandle, Error>;

    /// Opens a file inside a file set. If the handle is read-only, and the file does not exists,
    /// it blocks until it is created. If the handle is a writing handle, it creates the file.
    /// Returns an error if the handle is invalid.
    async fn open_file(handle: FileSetHandle, file: FileSetFile) -> Result<FileHandle, Error>;

    /// Appends data to a file in a fileset that is open for writing. Refreshes the writing lease.
    async fn append_chunk(file: FileHandle, data: Vec<u8>) -> Result<(), Error>;

    /// Finalizes a FileSet handle in writing mode. Terminates the writing lease and returns a reading
    /// lease for the same FileSet. If finalizing an input fileset, returns an error if the hash of
    /// its MainFile is not correct.
    async fn finalize_file_set(handle: FileSetHandle) -> Result<FileSetHandle, Error>;

    /// Tries to read from a file. Refreshes the corresponding lease.
    async fn read_chunk(file: FileHandle, offset: usize) -> Result<FileReadingOutcome, Error>;

    /// Refreshes the lease for the given fileset.
    /// It is guaranteed that the fileset will not be deleted while there's an outstanding lease
    /// to it.
    async fn refresh_file_set_lease(handle: FileSetHandle) -> Result<(), Error>;
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
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

type FileSetVariants = HashMap<VariantIdentificationHash, FileSet>;

#[derive(Debug)]
struct FileSetHandleInfo {
    data_hash: DataIdentificationHash,
    variant_hash: VariantIdentificationHash,
    expiration: Instant,
    mode: HandleMode,
    file_handles: HashMap<FileHandleId, FileSetFile>,
    main_file_hasher: Option<Hasher>,
    next_handle: FileHandleId,
}

struct StoreServiceImpl {
    file_sets: HashMap<DataIdentificationHash, FileSetVariants>,
    file_set_handles: HashMap<FileSetHandleId, FileSetHandleInfo>,
    next_handle: FileSetHandleId,
}

impl StoreServiceImpl {
    fn new() -> Self {
        StoreServiceImpl {
            file_sets: HashMap::new(),
            file_set_handles: HashMap::new(),
            next_handle: 0,
        }
    }

    fn new_handle(
        &mut self,
        hash: FileSetHash,
        mode: HandleMode,
        kind: FileSetKind,
    ) -> FileSetHandle {
        let handle = self.next_handle;
        self.next_handle += 1;
        let FileSetHash(data_hash, variant_hash) = hash;
        let handle_info = FileSetHandleInfo {
            data_hash,
            variant_hash,
            expiration: Instant::now() + LEASE_LENGTH,
            mode,
            file_handles: HashMap::new(),
            main_file_hasher: if kind == FileSetKind::InputFile {
                Some(Hasher::new())
            } else {
                None
            },
            next_handle: 0,
        };
        self.file_set_handles.insert(handle, handle_info);
        FileSetHandle { id: handle, mode }
    }

    fn create_or_open_file_set(
        &mut self,
        hash: FileSetHash,
        kind: FileSetKind,
    ) -> Result<FileSetHandle, Error> {
        let data_comp = self.file_sets.entry(hash.0).or_default();
        if let Some(file_set) = data_comp.get(&hash.1) {
            if file_set.kind != kind {
                return Err(Error::HashCollision(hash.0));
            }
            Ok(self.new_handle(hash, HandleMode::Read, kind))
        } else {
            data_comp.insert(
                hash.1,
                FileSet {
                    files: HashMap::new(),
                    kind,
                    finalized: false,
                },
            );
            Ok(self.new_handle(hash, HandleMode::Write, kind))
        }
    }

    /// Clear all the writing leases that have expired, removing the corresponding files.
    fn clear_expired_leases(&mut self) {
        let time = Instant::now();
        for info in self.file_set_handles.values() {
            if info.expiration < time && info.mode == HandleMode::Write {
                self.file_sets
                    .get_mut(&info.data_hash)
                    .unwrap()
                    .remove(&info.variant_hash);
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
        let file_set_info = self
            .file_set_handles
            .get_mut(&file.file_set_handle.id)
            .unwrap();
        let file_info = file_set_info.file_handles.get(&file.id).unwrap();
        let file_set = self
            .file_sets
            .get_mut(&file_set_info.data_hash)
            .unwrap()
            .get_mut(&file_set_info.variant_hash)
            .unwrap();

        if *file_info == FileSetFile::MainFile {
            if let Some(hasher) = file_set_info.main_file_hasher.as_mut() {
                hasher.update(&data);
            }
        }

        file_set.files.get_mut(file_info).unwrap().append(&mut data);
    }

    fn open_file(
        &mut self,
        file_set_handle: FileSetHandle,
        file: FileSetFile,
    ) -> Result<FileHandle, Error> {
        let file_set_info = self.file_set_handles.get_mut(&file_set_handle.id).unwrap();
        let file_set_group = self.file_sets.get_mut(&file_set_info.data_hash);
        if file_set_group.is_none() {
            return Err(Error::FileSetDropped(file_set_handle.id));
        }
        let file_set = file_set_group.unwrap().get_mut(&file_set_info.variant_hash);
        if file_set.is_none() {
            return Err(Error::FileSetDropped(file_set_handle.id));
        }
        let file_set = file_set.unwrap();
        if !file_set.finalized && file_set_handle.mode == HandleMode::Read {
            return Err(Error::NotImplemented(
                "Opening a file for reading in a non-finalized file_set is not implemented".into(),
            ));
        }
        if !file_set.files.contains_key(&file) {
            if file_set_handle.mode == HandleMode::Read {
                return Err(Error::NonExistentFile(file, file_set_handle.id));
            } else if file_set.kind == FileSetKind::InputFile
                && matches!(file, FileSetFile::AuxiliaryFile(_))
            {
                return Err(Error::InvalidFileForInput(file));
            } else {
                file_set.files.insert(file.clone(), vec![]);
            }
        } else if file_set_handle.mode == HandleMode::Write {
            return Err(Error::MultipleWrites(file, file_set_handle.id));
        }
        let file_handle = file_set_info.next_handle;
        file_set_info.next_handle += 1;
        file_set_info.file_handles.insert(file_handle, file);
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
        let file_set_info = self.file_set_handles.get(&file.file_set_handle.id).unwrap();
        let file_info = file_set_info.file_handles.get(&file.id).unwrap();
        let file_set_group = self.file_sets.get(&file_set_info.data_hash);
        if file_set_group.is_none() {
            return Ok(FileReadingOutcome::Dropped);
        }
        let file_set = file_set_group.unwrap().get(&file_set_info.variant_hash);
        if file_set.is_none() {
            return Ok(FileReadingOutcome::Dropped);
        }
        let file_set = file_set.unwrap();
        if !file_set.finalized {
            return Err(Error::NotImplemented(
                "Reading from a non-finalized file_set is not implemented".into(),
            ));
        }
        let file = file_set.files.get(file_info).unwrap();
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
    pub fn create_computation(
        &self,
        computation: DataIdentificationHash,
        variant: VariantIdentificationHash,
    ) -> Result<FileSetHandle, Error> {
        let mut service = self.service.lock().unwrap();
        let hash = FileSetHash(computation, variant);
        let handle = service.create_or_open_file_set(hash, FileSetKind::Computation)?;
        if handle.is_writable() {
            Ok(handle)
        } else {
            Err(Error::ComputationExists(computation, variant))
        }
    }

    /// Lists all the finalized variants of the given computation.
    pub fn list_variants(
        &self,
        computation: DataIdentificationHash,
    ) -> Vec<VariantIdentificationHash> {
        let service = self.service.lock().unwrap();
        match service.file_sets.get(&computation) {
            Some(variants) => variants
                .iter()
                .filter_map(|(k, v)| if v.finalized { Some(k) } else { None })
                .cloned()
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
        hash: DataIdentificationHash,
    ) -> Result<FileSetHandle, Error> {
        let mut service = self.service.lock().unwrap();
        service.create_or_open_file_set(FileSetHash(hash, hash), FileSetKind::InputFile)
    }

    async fn open_computation(
        self,
        context: Context,
        computation: DataIdentificationHash,
        variant: VariantIdentificationHash,
    ) -> Result<FileSetHandle, Error> {
        let has_comp = {
            let service = self.service.lock().unwrap();
            service
                .file_sets
                .get(&computation)
                .and_then(|x| x.get(&variant))
                .is_some()
        };

        if !has_comp {
            return Err(Error::NotImplemented(
                "Waiting for computation creation is not yet implemented".into(),
            ));
        }

        let mut service = self.service.lock().unwrap();
        let file_set = service
            .file_sets
            .get(&computation)
            .and_then(|x| x.get(&variant))
            .unwrap();
        if file_set.kind != FileSetKind::Computation {
            return Err(Error::HashCollision(computation));
        }
        Ok(service.new_handle(
            FileSetHash(computation, variant),
            HandleMode::Read,
            FileSetKind::Computation,
        ))
    }

    async fn finalize_file_set(
        self,
        context: Context,
        handle: FileSetHandle,
    ) -> Result<FileSetHandle, Error> {
        if handle.mode == HandleMode::Read {
            return Err(Error::FinalizeRead(handle.id));
        }
        let mut service = self.service.lock().unwrap();
        let service = &mut *service;

        {
            let entry = service.validate_and_refresh(handle)?;
            entry
                .and_modify(|v| v.mode = HandleMode::Read)
                .and_modify(|v| v.file_handles.clear());
        }

        let handle_info = service.file_set_handles.get_mut(&handle.id).unwrap();

        if let Some(hasher) = handle_info.main_file_hasher.take() {
            let hash = hasher.finalize();
            if *hash.as_bytes() != handle_info.data_hash {
                return Err(Error::InvalidHash(handle_info.data_hash, *hash.as_bytes()));
            }
        }

        service
            .file_sets
            .get_mut(&handle_info.data_hash)
            .unwrap()
            .get_mut(&handle_info.variant_hash)
            .unwrap()
            .finalized = true;

        Ok(FileSetHandle {
            id: handle.id,
            mode: HandleMode::Read,
        })
    }

    async fn refresh_file_set_lease(
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

    fn get_hash(data: &str) -> HashData {
        *blake3::hash(data.as_bytes()).as_bytes()
    }

    #[tokio::test]
    async fn test_write_and_read_files() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();

        let data = "input1";
        let hash = get_hash(data);
        let resp = client.create_or_open_input_file(context, hash).await?;
        let_assert!(Ok(fileset_handle) = resp);
        check!(fileset_handle.is_writable());

        for (i, file_type) in [(0, FileSetFile::MainFile), (1, FileSetFile::Metadata)] {
            let resp = client
                .open_file(context, fileset_handle, file_type.clone())
                .await?;
            let_assert!(Ok(file_handle) = resp);

            let resp = if file_type == FileSetFile::MainFile {
                client
                    .append_chunk(context, file_handle, data.as_bytes().to_vec())
                    .await?
            } else {
                let resp = client
                    .append_chunk(context, file_handle, vec![i, i, i])
                    .await?;
                let_assert!(Ok(()) = resp);

                client
                    .append_chunk(context, file_handle, vec![42, 42])
                    .await?
            };
            let_assert!(Ok(()) = resp);
        }

        let resp = client.finalize_file_set(context, fileset_handle).await?;
        let_assert!(Ok(fileset_handle) = resp);
        check!(!fileset_handle.is_writable());

        let resp = client.create_or_open_input_file(context, hash).await?;
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
        assert!(data1[..] == *data.as_bytes());

        let resp = client.read_chunk(context, file_handle2, 0).await?;
        let_assert!(Ok(FileReadingOutcome::Data(data2)) = resp);
        assert!(data2[..] == *data.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn test_read_not_existent() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = server.create_computation(hash, hash).unwrap();
        let file = client
            .open_file(
                context,
                write_handle,
                FileSetFile::AuxiliaryFile("file".into()),
            )
            .await?
            .unwrap();
        client
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client.open_computation(context, hash, hash).await?.unwrap();
        check!(!read_handle.is_writable());

        let file = FileSetFile::AuxiliaryFile("lolnope".into());
        let resp = client.open_file(context, read_handle, file.clone()).await?;
        let_assert!(Err(Error::NonExistentFile(file, _)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_write_with_readonly() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let data = "comp1";
        let hash = get_hash(data);
        let write_handle = client
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let file = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .append_chunk(context, file, data.as_bytes().to_vec())
            .await?
            .unwrap();
        client
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, hash)
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
            .create_or_open_input_file(context, hash)
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
        let data = "comp1";
        let hash = get_hash(data);
        let write_handle = client
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let file_handle = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .append_chunk(context, file_handle, data.as_bytes().to_vec())
            .await?
            .unwrap();
        client
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        check!(!read_handle.is_writable());
        let resp = client.finalize_file_set(context, read_handle).await?;
        let_assert!(Err(Error::FinalizeRead(_)) = resp);
        Ok(())
    }

    #[tokio::test]
    async fn test_no_auxiliary_file_for_input() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = client
            .create_or_open_input_file(context, hash)
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
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let write_handle = server.create_computation(hash, hash).unwrap();
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
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client.open_computation(context, hash, hash).await?.unwrap();
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
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let read_handle = client
            .create_or_open_input_file(context, hash)
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
                .refresh_file_set_lease(context, read_handle)
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
            .create_or_open_input_file(context, hash)
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
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let read_handle = client
            .create_or_open_input_file(context, hash)
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
            .create_or_open_input_file(context, hash)
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
        let data = "comp1";
        let hash = get_hash(data);
        let write_handle = client
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let file_handle = client
            .open_file(context, write_handle, FileSetFile::MainFile)
            .await?
            .unwrap();
        client
            .append_chunk(context, file_handle, data.as_bytes().to_vec())
            .await?
            .unwrap();
        client
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let read_handle = client
            .create_or_open_input_file(context, hash)
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
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();

        tokio::time::sleep(LEASE_LENGTH * 2).await;
        // now the lease is expired

        let resp = client.refresh_file_set_lease(context, write_handle).await?;
        let_assert!(Err(Error::UnknownHandle(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let input_file_handle = client
            .create_or_open_input_file(context, hash)
            .await?
            .unwrap();
        let resp = client.open_computation(context, hash, hash).await?;
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision2() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        server.create_computation(hash, hash).unwrap();
        let resp = client.create_or_open_input_file(context, hash).await?;
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_computation_exists() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        server.create_computation(hash, hash).unwrap();
        let resp = server.create_computation(hash, hash);
        let_assert!(Err(Error::ComputationExists(hash1, hash2)) = resp);
        assert!(hash1 == hash);
        assert!(hash2 == hash);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // because waiting for computation creation is not implemented yet
    async fn test_wait_computation_creation() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let mut fut = Box::pin(client.open_computation(context, hash, hash));
        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            resp = &mut fut => panic!("should be waiting for the computation, but got: {:?}", resp),
        }

        let comp = server.create_computation(hash, hash).unwrap();

        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => panic!("the computation should be ready now"),
            resp = fut => assert!(let Ok(_) = resp),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_list_variants() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let data1 = get_hash("data1");
        let data2 = get_hash("data2");
        let variant1 = get_hash("variant1");
        let variant2 = get_hash("variant2");
        for data in [data1, data2] {
            for variant in [variant1, variant2] {
                let handle = server.create_computation(data, variant).unwrap();
                client
                    .finalize_file_set(context, handle)
                    .await
                    .unwrap()
                    .unwrap();
            }
        }

        let mut similar = server.list_variants(data1);
        let mut expected = vec![variant1, variant2];
        similar.sort_unstable();
        expected.sort_unstable();
        assert!(similar == expected);

        let similar = server.list_variants(get_hash("lolnope"));
        assert!(similar.is_empty());

        Ok(())
    }
}
