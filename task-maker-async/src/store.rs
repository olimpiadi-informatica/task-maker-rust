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

#[derive(Debug, Serialize, Deserialize)]
pub struct InputFileHash(HashData);

/// Two-level hash; the outer hash has information about all properties of a computation that are
/// expected to change the result (such as outer hashes of inputs, or the command line). The inner
/// hash takes care of properties of the computation that should not change the outputs, such as
/// time and memory limits; it also includes the first hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputationHash(HashData, HashData);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
enum HandleMode {
    Read,
    Write,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileSetHandle {
    id: usize,
    mode: HandleMode,
}

impl FileSetHandle {
    pub fn is_writable(&self) -> bool {
        self.mode == HandleMode::Write
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileHandle {
    file_set_handle: FileSetHandle,
    id: usize,
}

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
    async fn read_chunk(file: FileHandle, offset: usize) -> FileReadingOutcome;

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

#[derive(Debug)]
struct FileSet {
    files: HashMap<FileSetFile, Vec<u8>>,
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
    file_handles: HashMap<usize, FileSetFile>,
    next_handle: usize,
}

struct StoreServiceImpl {
    filesets: HashMap<HashData, FileSetVariants>,
    fileset_handles: HashMap<usize, FileSetHandleInfo>,
    next_handle: usize,
}

impl StoreServiceImpl {
    fn new() -> Self {
        StoreServiceImpl {
            filesets: HashMap::new(),
            fileset_handles: HashMap::new(),
            next_handle: 0,
        }
    }

    fn new_handle(&mut self, mode: HandleMode) -> FileSetHandle {
        let handle = self.next_handle;
        self.next_handle += 1;
        FileSetHandle { id: handle, mode }
    }

    fn create_or_open_fileset(
        &mut self,
        hash: ComputationHash,
        kind: FileSetKind,
    ) -> Result<FileSetHandle, Error> {
        let outer_comp = self
            .filesets
            .entry(hash.0.clone())
            .or_insert_with(HashMap::new);
        let handle = if let Some(file_set) = outer_comp.get(&hash.1) {
            if file_set.kind != kind {
                return Err(Error::HashCollision(hash));
            }
            self.new_handle(HandleMode::Read)
        } else {
            outer_comp.insert(
                hash.1.clone(),
                FileSet {
                    files: HashMap::new(),
                    kind,
                    finalized: false,
                },
            );
            self.new_handle(HandleMode::Write)
        };
        let ComputationHash(outer_hash, inner_hash) = hash;
        let handle_info = FileSetHandleInfo {
            outer_hash,
            inner_hash,
            expiration: Instant::now() + LEASE_LENGTH,
            mode: handle.mode,
            file_handles: HashMap::new(),
            next_handle: 0usize,
        };
        self.fileset_handles.insert(handle.id, handle_info);
        Ok(handle)
    }
}

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

    pub fn clear_expired_leases(&self) {
        let mut service_guard = self.service.lock().unwrap();
        let service = &mut *service_guard;
        let time = Instant::now();
        for info in service.fileset_handles.values() {
            if info.expiration >= time && info.mode == HandleMode::Write {
                service
                    .filesets
                    .get_mut(&info.outer_hash)
                    .unwrap()
                    .remove(&info.inner_hash);
            }
        }
        service
            .fileset_handles
            .retain(|_, info| info.expiration >= time);
    }

    /// Creates and starts a store, listening for RPCs on the given transport.
    pub fn new_on_transport(
        transport: impl Transport<Response<StoreResponse>, ClientMessage<StoreRequest>> + Send + 'static,
    ) -> Self {
        let token = CancellationToken::new();

        let s = StoreService {
            service: Arc::new(Mutex::new(StoreServiceImpl::new())),
            cancellation_token: Arc::new(token),
        };
        let server = BaseChannel::with_defaults(transport);
        let s_clone = s.clone();

        let token = s.cancellation_token.clone();
        tokio::spawn(async move {
            select! {
                _ = token.cancelled() => {}
                _ = server.execute(s_clone.serve()) => { }
            }
        });

        let token = s.cancellation_token.clone();
        let s_clone = s.clone();
        tokio::spawn(async move {
            let mut timer = interval(LEASE_LENGTH);
            let timer = async {
                loop {
                    let x = timer.tick().await;
                    s_clone.clear_expired_leases();
                }
            };

            select! {
                _ = token.cancelled() => {
                    println!("Cancelled");
                }
                _ = timer => { }
            }
        });
        s
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
        let get_handle = || {
            let hash = hash.clone();
            let mut service = self.service.lock().unwrap();
            let outer_comp = service
                .filesets
                .entry(hash.0.clone())
                .or_insert_with(HashMap::new);
            if let Some(file_set) = outer_comp.get(&hash.1) {
                if file_set.kind != FileSetKind::Computation {
                    return Err(Error::HashCollision(hash));
                }
                Ok(Some(service.new_handle(HandleMode::Read)))
            } else {
                Ok(None)
            }
        };
        #[allow(clippy::never_loop)]
        let handle = loop {
            let handle = get_handle()?;
            if let Some(handle) = handle {
                break handle;
            } else {
                return Err(Error::NotImplemented(
                    "waiting for computation creation is not yet implemented".into(),
                ));
            }
        };
        let handle_info = FileSetHandleInfo {
            outer_hash: hash.0.clone(),
            inner_hash: hash.1.clone(),
            expiration: Instant::now() + LEASE_LENGTH,
            mode: HandleMode::Read,
            file_handles: HashMap::new(),
            next_handle: 0usize,
        };
        self.service
            .lock()
            .unwrap()
            .fileset_handles
            .insert(handle.id, handle_info);
        Ok(handle)
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
        let entry = service.fileset_handles.entry(handle.id);
        if let Entry::Vacant(_) = entry {
            return Err(Error::UnknownHandle(handle.id));
        }
        if let Entry::Occupied(entry) = &entry {
            if entry.get().mode != handle.mode {
                return Err(Error::UnknownHandle(handle.id));
            }
        }
        // TODO(veluca): we should verify the hash of the file if this is a fileset.
        entry.and_modify(|v| v.expiration = Instant::now() + LEASE_LENGTH);
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
        let entry = service.fileset_handles.entry(handle.id);
        match entry {
            Entry::Vacant(_) => Err(Error::UnknownHandle(handle.id)),
            Entry::Occupied(mut entry) => {
                let mut v = entry.get_mut();
                if v.mode != handle.mode {
                    Err(Error::UnknownHandle(handle.id))
                } else {
                    v.expiration = Instant::now() + LEASE_LENGTH;
                    Ok(())
                }
            }
        }
    }

    async fn open_file(
        self,
        context: Context,
        handle: FileSetHandle,
        file: FileSetFile,
    ) -> Result<FileHandle, Error> {
        Err(Error::NotImplemented("open_file".into()))
    }

    async fn append_chunk(
        self,
        context: Context,
        file: FileHandle,
        data: Vec<u8>,
    ) -> Result<(), Error> {
        Err(Error::NotImplemented("append".into()))
    }

    async fn read_chunk(
        self,
        context: Context,
        file: FileHandle,
        offset: usize,
    ) -> FileReadingOutcome {
        todo!("not implemented");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tarpc::context;

    #[tokio::test]
    async fn create() {
        let (client_transport, server_transport) = tarpc::transport::channel::unbounded();
        let client = StoreClient::new(tarpc::client::Config::default(), client_transport).spawn();
        let context = context::current();
        let server = StoreService::new_on_transport(server_transport);
        let resp = client
            .create_or_open_input_file(context, InputFileHash(vec![]))
            .await
            .unwrap();
        println!("{:?}", resp);
        let resp2 = client
            .create_or_open_input_file(context, InputFileHash(vec![]))
            .await
            .unwrap();
        println!("{:?}", resp2);
        let resp3 = client
            .refresh_fileset_lease(context::current(), resp.unwrap())
            .await
            .unwrap();
        println!("{:?}", resp3);
        tokio::time::sleep(LEASE_LENGTH + Duration::from_millis(100)).await;
        let resp4 = client
            .refresh_fileset_lease(context::current(), resp2.unwrap())
            .await
            .unwrap();
        println!("{:?}", resp4);
        server.stop();
    }
}
