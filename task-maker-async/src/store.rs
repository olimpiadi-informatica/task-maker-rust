#![allow(dead_code)]

use std::collections::HashMap;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::{ClientMessage, Response, Transport};
use tokio::select;

use tokio_util::sync::CancellationToken;

use crate::error::Error;
use crate::file_set::{FileReadingOutcome, FileSet, FileSetFile, FileSetKind};

type HashData = [u8; 32];

/// Maximum amount of time that a write request will wait for activate_for_writing.
// TODO(veluca): if a FileSet gets created / put into waiting status and nobody ever calls
// activate_for_writing() (for instance because the worker that should have done so crashed), the
// FileSet will remain present and the waiters never notified. This should be fixed, i.e. by adding
// a time limit between FileSet creation and calls to activate_for_writing().
const ACTIVATION_MAX_WAITING_TIME: Duration = Duration::from_secs(30);

const CHUNK_SIZE: usize = 4 * 1024; // 4 KiB

/// Hash that uniquely identifies the *content* of a given fileset.
pub type DataIdentificationHash = HashData;

/// Hash that uniquely identifies a *variant* of a given fileset.
pub type VariantIdentificationHash = HashData;

/// Two-level hash; the DataIdentificationHash has information about all properties of a fileset
/// that are expected to change the result (such as data hashes of inputs, or the command line).
/// The VariantIdentificationHash takes care of properties of the computation that should not
/// change the outputs, such as time and memory limits; it also includes the first hash.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileSetHash {
    pub data: DataIdentificationHash,
    pub variant: VariantIdentificationHash,
}

pub type FileSetHandleId = usize;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct FileSetWriteHandle {
    id: FileSetHandleId,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum WaitFor {
    Creation,
    Finalization,
}

#[tarpc::service]
pub trait Store {
    /// Used by the client to upload inputs to the DAG. Returns a writing handle if the input file is
    /// not present, or None if it is.
    async fn create_input_file(
        hash: DataIdentificationHash,
    ) -> Result<Option<FileSetWriteHandle>, Error>;

    // Creating a computation is not a RPC.

    /// Activate a writing handle, allowing writing to start. This RPC returns when writing can be
    /// stopped, either because writing has completed (in which case it returns true) or because
    /// the computation has been dropped (in which case it returns false). If the RPC is dropped
    /// from the client side, the file set will be dropped on the server side and further writes
    /// will fail.
    async fn activate_for_writing(handle: FileSetWriteHandle) -> Result<bool, Error>;

    /// Appends data to a file in a fileset.
    async fn append_chunk(
        handle: FileSetWriteHandle,
        file: FileSetFile,
        data: Vec<u8>,
    ) -> Result<(), Error>;

    /// Finalizes a FileSet. Terminates the writing handle. If finalizing an
    /// input fileset, returns an error if the hash of its MainFile is not correct.
    async fn finalize_file_set(handle: FileSetWriteHandle) -> Result<(), Error>;

    /// Drops a computation if it is not yet finalized. If the variant does not exist anymore, or
    /// the computation was finalized, does nothing.
    async fn drop_computation(hash: FileSetHash) -> Result<(), Error>;

    /// Waits until the given fileset is created (or finalized).
    async fn wait_for_fileset(hash: FileSetHash, wait_for: WaitFor) -> Result<(), Error>;

    /// Read from a file. If the file is not yet ready, waits until the file is ready. Note:
    /// reading from a nonexistent file may result either in a Dropped or an immediate EndOfFile
    /// response.
    async fn read_chunk(
        file_set_hash: FileSetHash,
        file: FileSetFile,
        offset: usize,
    ) -> Result<FileReadingOutcome, Error>;
}

type FileSetVariants = HashMap<VariantIdentificationHash, FileSet>;

#[derive(Debug)]
struct FileSetHandleInfo {
    hash: FileSetHash,
    input_file_hasher: Option<Hasher>,
}

struct StoreServiceImpl {
    file_sets: HashMap<DataIdentificationHash, FileSetVariants>,
    handles: HashMap<FileSetHandleId, FileSetHandleInfo>,
    next_handle: FileSetHandleId,
}

impl StoreServiceImpl {
    fn new() -> Self {
        StoreServiceImpl {
            file_sets: HashMap::new(),
            handles: HashMap::new(),
            next_handle: 0,
        }
    }

    fn new_handle(&mut self, hash: FileSetHash, kind: FileSetKind) -> FileSetWriteHandle {
        let handle = self.next_handle;
        self.next_handle += 1;
        let handle_info = FileSetHandleInfo {
            hash,
            input_file_hasher: if kind == FileSetKind::InputFile {
                Some(Hasher::new())
            } else {
                None
            },
        };
        self.handles.insert(handle, handle_info);
        FileSetWriteHandle { id: handle }
    }

    fn create_if_not_exists(
        &mut self,
        hash: FileSetHash,
        kind: FileSetKind,
    ) -> Result<Option<FileSetWriteHandle>, Error> {
        let data_comp = self.file_sets.entry(hash.data).or_default();
        let file_set = data_comp.entry(hash.variant).or_insert_with(FileSet::new);
        if file_set
            .create(kind)
            .map_err(|()| Error::HashCollision(hash))?
        {
            Ok(Some(self.new_handle(hash, kind)))
        } else {
            Ok(None)
        }
    }

    fn get_handle_info(
        &mut self,
        handle: &FileSetWriteHandle,
    ) -> Result<&mut FileSetHandleInfo, Error> {
        self.handles
            .get_mut(&handle.id)
            .ok_or(Error::UnknownHandle(handle.id))
    }

    fn get_fileset(&mut self, hash: FileSetHash) -> Option<&mut FileSet> {
        self.file_sets
            .get_mut(&hash.data)
            .and_then(|x| x.get_mut(&hash.variant))
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
    /// a writing handle that the workers can use. Creating a computation if
    /// another computation with the same hash already exists, even if it is temporary (i.e. not
    /// finalized), will result in this method returning None.
    pub fn create_computation(
        &self,
        hash: FileSetHash,
    ) -> Result<Option<FileSetWriteHandle>, Error> {
        let mut service = self.service.lock().unwrap();
        service.create_if_not_exists(hash, FileSetKind::Computation)
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
                .filter_map(|(k, v)| if v.is_finalized() { Some(k) } else { None })
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

        service
    }

    /// Stops the server.
    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }
}

#[tarpc::server]
impl Store for StoreService {
    async fn create_input_file(
        self,
        _context: Context,
        hash: DataIdentificationHash,
    ) -> Result<Option<FileSetWriteHandle>, Error> {
        let mut service = self.service.lock().unwrap();
        service.create_if_not_exists(
            FileSetHash {
                data: hash,
                variant: hash,
            },
            FileSetKind::InputFile,
        )
    }

    async fn activate_for_writing(
        self,
        _context: Context,
        handle: FileSetWriteHandle,
    ) -> Result<bool, Error> {
        let hash;
        let fileset_finalized = {
            let mut service = self.service.lock().unwrap();
            hash = service.get_handle_info(&handle)?.hash;
            let file_set = service.get_fileset(hash).unwrap();
            file_set.start_writing();
            file_set.wait_for_finalization()
        };
        // If this RPC is dropped before finalization (likely because the entity creating the file
        // crashed), drop the partial fileset.
        let dropped = AtomicBool::new(true);
        let _guard = scopeguard::guard((), |_| {
            if !dropped.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let mut service = self.service.lock().unwrap();
            service
                .file_sets
                .get_mut(&hash.data)
                .unwrap()
                .remove(&hash.variant);
        });
        let res = fileset_finalized.await;
        dropped.store(false, std::sync::atomic::Ordering::SeqCst);
        // Ok = fileset was finalized. Err = fileset was dropped.
        Ok(res.is_ok())
    }

    async fn append_chunk(
        self,
        _context: Context,
        handle: FileSetWriteHandle,
        file: FileSetFile,
        data: Vec<u8>,
    ) -> Result<(), Error> {
        let writable = {
            let mut service = self.service.lock().unwrap();
            let hash = service.get_handle_info(&handle)?.hash;
            let file_set = service.get_fileset(hash).unwrap();
            file_set.wait_for_writable()
        };
        tokio::select! {
            _ = writable => {},
            _ = tokio::time::sleep(ACTIVATION_MAX_WAITING_TIME) => {
                return Err(Error::NotActive(handle.id));
            }
        };
        let mut service = self.service.lock().unwrap();
        let handle_info = service.get_handle_info(&handle)?;
        if let Some(hasher) = &mut handle_info.input_file_hasher {
            if file == FileSetFile::MainFile {
                hasher.update(&data);
            }
        }
        let hash = handle_info.hash;
        let file_set = service.get_fileset(hash).unwrap();
        if file_set.kind() == FileSetKind::InputFile
            && matches!(file, FileSetFile::AuxiliaryFile(_, _))
        {
            return Err(Error::InvalidFileForInput(file));
        }
        file_set.append_to_file(&file, &data);
        Ok(())
    }

    async fn finalize_file_set(
        self,
        _context: Context,
        handle: FileSetWriteHandle,
    ) -> Result<(), Error> {
        let mut service = self.service.lock().unwrap();
        let handle_info = service.get_handle_info(&handle)?;
        if let Some(hasher) = handle_info.input_file_hasher.take() {
            let hash = hasher.finalize();
            if *hash.as_bytes() != handle_info.hash.data {
                return Err(Error::InvalidHash(handle_info.hash.data, *hash.as_bytes()));
            }
        }
        let hash = handle_info.hash;
        let file_set = service.get_fileset(hash).unwrap();
        file_set.mark_finalized();
        service.handles.remove(&handle.id);
        Ok(())
    }

    async fn drop_computation(self, _context: Context, hash: FileSetHash) -> Result<(), Error> {
        let mut service = self.service.lock().unwrap();
        let data_hm = service
            .file_sets
            .get_mut(&hash.data)
            .ok_or(Error::UnknownHash(hash))?;
        if let Some(fs) = data_hm.get_mut(&hash.variant) {
            if fs.is_finalized() {
                return Ok(());
            }
            data_hm.remove(&hash.variant).unwrap();
        }
        Ok(())
    }

    async fn wait_for_fileset(
        self,
        _context: Context,
        hash: FileSetHash,
        wait_for: WaitFor,
    ) -> Result<(), Error> {
        let res = match wait_for {
            WaitFor::Finalization => {
                let fileset_finalized = {
                    let mut service = self.service.lock().unwrap();
                    let data_comp = service.file_sets.entry(hash.data).or_default();
                    let file_set = data_comp.entry(hash.variant).or_insert_with(FileSet::new);
                    file_set.wait_for_finalization()
                };
                fileset_finalized.await
            }
            WaitFor::Creation => {
                let fileset_exists = {
                    let mut service = self.service.lock().unwrap();
                    let data_comp = service.file_sets.entry(hash.data).or_default();
                    let file_set = data_comp.entry(hash.variant).or_insert_with(FileSet::new);
                    file_set.wait_for_creation()
                };
                fileset_exists.await
            }
        };
        // If waiting produced errors, they were caused by the fileset being dropped.
        res.map_err(|_| Error::FileSetDropped(hash))
    }

    async fn read_chunk(
        self,
        _context: Context,
        file_set_hash: FileSetHash,
        file: FileSetFile,
        offset: usize,
    ) -> Result<FileReadingOutcome, Error> {
        let has_read_result = {
            let mut service = self.service.lock().unwrap();
            let file_set = service
                .get_fileset(file_set_hash)
                .ok_or(Error::FileSetDropped(file_set_hash))?;
            file_set.read_from_file(&file, offset, CHUNK_SIZE)
        };
        Ok(has_read_result.await)
    }
}

#[cfg(test)]
mod test {
    use assert2::{assert, check, let_assert};
    use tarpc::client::RpcError;
    use tarpc::context;

    use crate::file_set::ExecutionFile;

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

    fn activate(client: &StoreClient, handle: FileSetWriteHandle) {
        let client_clone = client.clone();
        tokio::spawn(async move {
            client_clone
                .activate_for_writing(context::current(), handle)
                .await
                .unwrap()
                .unwrap();
        });
    }

    #[tokio::test]
    async fn test_write_and_read_files() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();

        let data = "input1";
        let hash = get_hash(data);
        let resp = client.create_input_file(context, hash).await?;
        let_assert!(Ok(Some(fileset_handle)) = resp);

        activate(&client, fileset_handle);

        for (i, file_type) in [(0, FileSetFile::MainFile), (1, FileSetFile::Metadata)] {
            let resp = if file_type == FileSetFile::MainFile {
                client
                    .append_chunk(context, fileset_handle, file_type, data.as_bytes().to_vec())
                    .await?
            } else {
                let resp = client
                    .append_chunk(context, fileset_handle, file_type.clone(), vec![i, i, i])
                    .await?;
                let_assert!(Ok(()) = resp);

                client
                    .append_chunk(context, fileset_handle, file_type, vec![42, 42])
                    .await?
            };
            let_assert!(Ok(()) = resp);
        }

        let resp = client.finalize_file_set(context, fileset_handle).await?;
        let_assert!(Ok(()) = resp);

        let resp = client.create_input_file(context, hash).await?;
        check!(matches!(resp, Ok(None)));

        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };

        let resp = client
            .read_chunk(context, hash, FileSetFile::MainFile, 0)
            .await?;
        let_assert!(Ok(FileReadingOutcome::Data(data_read1)) = resp);
        assert!(data_read1[..] == *data.as_bytes());

        let resp = client
            .read_chunk(context, hash, FileSetFile::MainFile, 0)
            .await?;
        let_assert!(Ok(FileReadingOutcome::Data(data_read2)) = resp);
        assert!(data_read2[..] == *data.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn test_read_not_existent() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        let write_handle = server.create_computation(hash).unwrap().unwrap();
        client
            .finalize_file_set(context, write_handle)
            .await?
            .unwrap();

        let file = FileSetFile::AuxiliaryFile("lolnope".into(), ExecutionFile::Stdout);
        let resp = client.read_chunk(context, hash, file, 0).await?;
        let_assert!(Ok(FileReadingOutcome::Dropped) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_no_auxiliary_file_for_input() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let handle = client
            .create_input_file(context, hash)
            .await?
            .unwrap()
            .unwrap();
        activate(&client, handle);
        let file = FileSetFile::AuxiliaryFile("execution".into(), ExecutionFile::Stdout);
        let resp = client
            .append_chunk(context, handle, file.clone(), vec![])
            .await?;
        let_assert!(Err(Error::InvalidFileForInput(_)) = resp);
        Ok(())
    }

    #[tokio::test]
    async fn test_chunked_read() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        let handle = server.create_computation(hash).unwrap().unwrap();
        activate(&client, handle);
        let file = FileSetFile::MainFile;
        let mut expected = vec![];
        // 5 full chunks
        for i in 0..5 {
            let mut chunk = vec![i; CHUNK_SIZE];
            client
                .append_chunk(context, handle, file.clone(), chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        // 5 small chunks
        for i in 0..5 {
            let mut chunk = vec![i; 3];
            client
                .append_chunk(context, handle, file.clone(), chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        // 5 full chunks
        for i in 0..5 {
            let mut chunk = vec![i; CHUNK_SIZE];
            client
                .append_chunk(context, handle, file.clone(), chunk.clone())
                .await?
                .unwrap();
            expected.append(&mut chunk);
        }
        client.finalize_file_set(context, handle).await?.unwrap();

        let mut data = vec![];
        loop {
            let outcome = client
                .read_chunk(context, hash, file.clone(), data.len())
                .await?
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
        assert_eq!(data, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_drop() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let handle = client
            .create_input_file(context, hash)
            .await?
            .unwrap()
            .unwrap();
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        activate(&client, handle);
        let file = FileSetFile::MainFile;

        client
            .append_chunk(context, handle, file.clone(), vec![1, 2, 3])
            .await?
            .unwrap();

        let client_clone = client.clone();
        let read = tokio::spawn(async move {
            let resp = client_clone
                .read_chunk(context, hash, file, 3)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(FileReadingOutcome::Dropped, resp);
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        client.drop_computation(context, hash).await?.unwrap();

        read.await.unwrap();

        Ok(())
    }

    #[tokio::test]
    async fn test_drop_terminates_activate() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let handle = client
            .create_input_file(context, hash)
            .await?
            .unwrap()
            .unwrap();
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        let file = FileSetFile::MainFile;

        let client_clone = client.clone();
        let mut activate = Box::pin(client_clone.activate_for_writing(context::current(), handle));
        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            resp = &mut activate => panic!("should be waiting for the computation to be dropped, but got: {:?}", resp),
        }

        client
            .append_chunk(context, handle, file.clone(), vec![1, 2, 3])
            .await?
            .unwrap();

        client.drop_computation(context, hash).await?.unwrap();

        assert!(let Ok(_) = activate.await);

        Ok(())
    }

    #[tokio::test]
    async fn test_read_disconnect() -> Result<(), RpcError> {
        let (client, context, _server) = spawn();
        let hash = get_hash("comp1");
        let handle = client
            .create_input_file(context, hash)
            .await?
            .unwrap()
            .unwrap();
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        let client_clone = client.clone();
        let mut activate = Box::pin(client_clone.activate_for_writing(context::current(), handle));
        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            resp = &mut activate => panic!("should be waiting for the computation to be dropped, but got: {:?}", resp),
        }
        let file = FileSetFile::MainFile;

        let client_clone = client.clone();
        let read = tokio::spawn(async move {
            let resp = client_clone
                .read_chunk(context, hash, file, 0)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(FileReadingOutcome::Dropped, resp);
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        drop(activate); // This deletes the file on the server side.

        read.await.unwrap();

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        client.create_input_file(context, hash).await?.unwrap();
        let resp = server.create_computation(FileSetHash {
            data: hash,
            variant: hash,
        });
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_hash_collision2() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        server
            .create_computation(FileSetHash {
                data: hash,
                variant: hash,
            })
            .unwrap();
        let resp = client.create_input_file(context, hash).await?;
        let_assert!(Err(Error::HashCollision(_)) = resp);

        Ok(())
    }

    #[tokio::test]
    async fn test_computation_exists() -> Result<(), RpcError> {
        let (_, _, server) = spawn();
        let hash = get_hash("comp1");
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        server.create_computation(hash).unwrap();
        let resp = server.create_computation(hash);
        assert!(matches!(resp, Ok(None)));

        Ok(())
    }

    #[tokio::test]
    async fn test_wait_computation_creation() -> Result<(), RpcError> {
        let (client, context, server) = spawn();
        let hash = get_hash("comp1");
        let hash = FileSetHash {
            data: hash,
            variant: hash,
        };
        let mut fut = Box::pin(client.wait_for_fileset(context, hash, WaitFor::Creation));
        select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            resp = &mut fut => panic!("should be waiting for the computation, but got: {:?}", resp),
        }

        server.create_computation(hash).unwrap();

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
                let handle = server
                    .create_computation(FileSetHash { data, variant })
                    .unwrap()
                    .unwrap();
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
