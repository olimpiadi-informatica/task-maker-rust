//! This crate manages the file store on disk, a folder with many files indexed by their hash.
//!
//! The files are stored in a read-only manner (removing the write bit permission) and their access
//! is granted via their hash. The size of the store folder is limited to a specific amount and the
//! least-recently-used files are removed automatically.
//!
//! The access to the store directory via this crate is exclusive even between processes.
//!
//! # Example
//!
//! Storing a file into the store and getting it back later.
//!
//! ```
//! use task_maker_store::{FileStore, FileStoreKey, ReadFileIterator};
//!
//! # use anyhow::Error;
//! # use std::fs;
//! # use tempdir::TempDir;
//! # fn main() -> Result<(), Error> {
//! # let tmp = TempDir::new("tm-test").unwrap();
//! # let store_dir = tmp.path().join("store");
//! # let path = tmp.path().join("file.txt");
//! # fs::write(&path, "hello world")?;
//! // make a new store based on a directory, this will lock if the store is already in use
//! let store = FileStore::new(store_dir, 1000, 1000)?;
//! // compute the key of a file and make an iterator over its content
//! let key = FileStoreKey::from_file(&path)?;
//! let iter = ReadFileIterator::new(&path)?;
//! // store the file inside the file store. The file will be kept at least until handle is alive
//! let handle = store.store(&key, iter)?;
//! // store.get(&key) will return an handle to the file on disk if present inside the store
//! assert!(store.get(&key).is_some());
//! # Ok(())
//! # }
//! ```

#![deny(missing_docs)]
#![allow(clippy::upper_case_acronyms)]

#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::fmt::Formatter;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Error};
use blake2::{Blake2b, Digest};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::index::FileStoreIndex;
pub use read_file_iterator::ReadFileIterator;

mod index;
mod read_file_iterator;

/// Whether to check the file integrity on the store before getting it.
const INTEGRITY_CHECKS_ENABLED: bool = false;
/// The name of the lock of the file store.
const STORE_LOCK_FILE: &str = "exclusive.lock";
/// The name of the index of the file store.
const STORE_INDEX_FILE: &str = "index.json";

/// The type of an hash of a file
type HashData = Vec<u8>;

/// Container with the ref counts of all the handles still alive.
#[derive(Debug)]
struct LockedFiles {
    /// Map from a `FileStoreKey` to the number of handles alive.
    ref_counts: HashMap<FileStoreKey, usize>,
}

/// A file store will manage all the files in the store directory.
///
/// This will manage a file storage directory with the ability of:
/// * remove files not needed anymore that takes too much space.
/// * locking so no other instances of `FileStorage` can access the storage while
///   this is still running, even in other processes.
/// * do not remove files useful for the current computations.
#[derive(Debug)]
pub struct FileStore {
    /// Base directory of the `FileStore`.
    base_path: PathBuf,
    /// Handle of the file with the data of the store. This handle keeps the lock alive.
    _file: File,
    /// The files locked because there are some handles still alive.
    locked_files: Arc<Mutex<LockedFiles>>,
    /// The index with the files known to the store. This is used when flushing the old files.
    index: Arc<Mutex<FileStoreIndex>>,
    /// Maximum size of the file store.
    max_store_size: u64,
    /// Target size of the file store after the flush.
    min_store_size: u64,
}

/// Handle of a file in the `FileStore`, this must be computable given the content of the file, i.e.
/// an hash of the content.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileStoreKey {
    /// An hash of the content of the file.
    hash: HashData,
}

/// An handle to a specific file inside the store, until this handle is dropped the `FileStore` will
/// not flush away the file for clearing space. You can clone the handle extending the life of the
/// file.
#[derive(Debug)]
pub struct FileStoreHandle {
    /// The key of the file inside the store.
    key: FileStoreKey,
    /// The path to the file on disk.
    path: PathBuf,
    /// A reference to the locked files. Will be used to remove self from the ref counts.
    locked_files: Arc<Mutex<LockedFiles>>,
}

impl FileStore {
    /// Make a new `FileStore` in the specified base directory, will lock if another instance of a
    /// `FileStore` is locking the directory. The locking is implemented via platform-specific file
    /// locking. Having two instances of the file store running concurrently is not safe.
    ///
    /// ```
    /// use task_maker_store::FileStore;
    ///
    /// # use anyhow::Error;
    /// # use std::fs;
    /// # use tempdir::TempDir;
    /// # fn main() -> Result<(), Error> {
    /// # let dir = TempDir::new("tm-test")?;
    /// # let store_dir = dir.path();
    /// // make a new store based on a directory, this will lock if the store is already in use
    /// // somewhere
    /// let store = FileStore::new(store_dir, 1000, 1000)?;
    /// // let store2 = FileStore::new(store_dir) // this will lock!!
    /// # Ok(())
    /// # }
    /// ```
    pub fn new<P: Into<PathBuf>>(
        base_path: P,
        max_store_size: u64,
        min_store_size: u64,
    ) -> Result<FileStore, Error> {
        let base_path = base_path.into();
        debug!("Opening file store at {}", base_path.display());
        std::fs::create_dir_all(&base_path).with_context(|| {
            format!(
                "Failed to create storage directory at {}",
                base_path.display()
            )
        })?;
        let lock = base_path.join(STORE_LOCK_FILE);
        let file = File::create(&lock)
            .with_context(|| format!("Failed to create lock file at {}", lock.display()))?;
        if let Err(e) = file.try_lock_exclusive() {
            if e.to_string() != fs2::lock_contended_error().to_string() {
                return Err(e.into());
            }
            warn!("Store locked... waiting");
            file.lock_exclusive()
                .context("Failed to obtain exclusive lock on storage")?;
        }
        let index = FileStoreIndex::load(base_path.join(STORE_INDEX_FILE))
            .context("Failed to load storage index")?;
        Ok(FileStore {
            base_path,
            _file: file,
            locked_files: Arc::new(Mutex::new(LockedFiles::new())),
            index: Arc::new(Mutex::new(index)),
            max_store_size,
            min_store_size,
        })
    }

    /// Given an iterator of `Vec<u8>` consume all of it writing the content to the disk if the file
    /// is not already present on disk. The file is stored inside the base directory and `chmod -w`.
    ///
    /// If the file is already present it is not overwritten but the iterator is consumed
    /// nevertheless.
    ///
    /// Will return an handle to that file, keeping the file alive.
    ///
    /// ```
    /// use task_maker_store::{FileStore, FileStoreKey, ReadFileIterator};
    ///
    /// # use anyhow::Error;
    /// # use std::fs;
    /// # use tempdir::TempDir;
    /// # fn main() -> Result<(), Error> {
    /// # let tmp = TempDir::new("tm-test").unwrap();
    /// # let store_dir = tmp.path().join("store");
    /// # let path = tmp.path().join("file.txt");
    /// # fs::write(&path, "hello world")?;
    /// let store = FileStore::new(store_dir, 1000, 1000)?;
    /// // compute the key of a file and make an iterator over its content
    /// let key = FileStoreKey::from_file(&path)?;
    /// let iter = ReadFileIterator::new(&path)?;
    /// // store the file inside the file store. The file will be kept at least until handle is alive
    /// let handle = store.store(&key, iter)?;
    /// // here it's guaranteed that the file won't be flushed away
    /// assert!(handle.path().exists());
    /// drop(handle);
    /// // here it's not guaranteed
    /// # Ok(())
    /// # }
    /// ```
    pub fn store<I>(&self, key: &FileStoreKey, content: I) -> Result<FileStoreHandle, Error>
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let path = self.key_to_path(key);
        trace!("Storing {:?}", path);
        // make the key to avoid racing while writing
        let handle = FileStoreHandle::new(self, key);
        if path.exists() {
            trace!("File {:?} already exists", path);
            content.into_iter().last(); // consume all the iterator
        } else {
            // assuming moving files is atomic this should be MT-safe
            let dir = path.parent().unwrap();
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Cannot create directory at {}", dir.display()))?;
            let tmpdir = tempdir::TempDir::new_in(path.parent().unwrap(), "temp")
                .context("Failed to create temporary directory for storing the file")?;
            let tmpfile_path = tmpdir.path().join("file");
            let mut tmpfile =
                std::fs::File::create(&tmpfile_path).context("Failed to create temporary file")?;
            if !content
                .into_iter()
                .map(|data| tmpfile.write_all(&data))
                .all(|r| r.is_ok())
            {
                bail!("Failed to store file");
            }
            std::fs::rename(&tmpfile_path, &path).with_context(|| {
                format!(
                    "Failed to rename {} -> {}",
                    tmpfile_path.display(),
                    path.display()
                )
            })?;
            FileStore::mark_readonly(&path).context("Failed to mark file as readonly")?;
            {
                let mut index = self.index.lock().unwrap();
                index
                    .add(key.clone(), path)
                    .context("Failed to add file to index")?;
                self.maybe_flush(&mut index)?;
                // FIXME: maybe this can be done less frequently
                index
                    .store(self.base_path.join(STORE_INDEX_FILE))
                    .context("Failed to store the index to file")?;
            }
        }
        Ok(handle)
    }

    /// Returns an handle to the file with that key or `None` if it's not in the
    /// [`FileStore`](struct.FileStore.html).
    ///
    /// This requires mutability because it will actively fix any corrupted or missing files in the
    /// store.
    ///
    /// The file is guaranteed to not be flushed until all the handles to it get dropped.
    ///
    /// ```
    /// use task_maker_store::{FileStore, FileStoreKey, ReadFileIterator};
    ///
    /// # use anyhow::Error;
    /// # use std::fs;
    /// # use tempdir::TempDir;
    /// # fn main() -> Result<(), Error> {
    /// # let tmp = TempDir::new("tm-test").unwrap();
    /// # let store_dir = tmp.path().join("store");
    /// # let path = tmp.path().join("file.txt");
    /// # fs::write(&path, "hello world")?;
    /// let store = FileStore::new(store_dir, 1000, 1000)?;
    /// let key = FileStoreKey::from_file(&path)?;
    /// # let iter = ReadFileIterator::new(&path)?;
    /// # let handle = store.store(&key, iter)?;
    /// let handle = store.get(&key);
    /// match handle {
    ///     None => panic!("The file is gone!"),
    ///     Some(handle) => assert!(handle.path().exists())
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&self, key: &FileStoreKey) -> Option<FileStoreHandle> {
        let path = self.key_to_path(key);
        if !path.exists() {
            return None;
        }
        if INTEGRITY_CHECKS_ENABLED && !self.check_integrity(key) {
            warn!("File {:?} failed the integrity check", path);
            if let Err(e) = FileStore::remove_file(&path) {
                warn!("Cannot remove corrupted file: {:?}", e);
            }
            return None;
        }
        {
            let mut index = self.index.lock().unwrap();
            index.touch(key);
        }
        Some(FileStoreHandle::new(self, key))
    }

    /// Path of the file to disk.
    fn key_to_path(&self, key: &FileStoreKey) -> PathBuf {
        self.base_path.join(key.suffix())
    }

    /// Mark a file as readonly.
    fn mark_readonly(path: &Path) -> Result<(), Error> {
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("Failed to get file metadata of {}", path.display()))?
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set permission of {}", path.display()))?;
        Ok(())
    }

    /// Remove a file from disk.
    fn remove_file(path: &Path) -> Result<(), Error> {
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("Failed to get file metadata of {}", path.display()))?
            .permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set permission of {}", path.display()))?;
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
        Ok(())
    }

    /// Check if the file is not corrupted.
    fn check_integrity(&self, key: &FileStoreKey) -> bool {
        let path = self.key_to_path(key);
        let metadata = std::fs::metadata(&path);
        // if the last modified time is the same of creation time assume it's
        // not corrupted
        if let Ok(metadata) = metadata {
            let created = metadata.created();
            let modified = metadata.modified();
            if let (Ok(created), Ok(modified)) = (created, modified) {
                if created == modified {
                    return true;
                }
            }
        }
        match FileStoreKey::from_file(&path) {
            Ok(key2) => key2.hash == key.hash,
            Err(_) => false,
        }
    }

    /// Check if the file store needs flushing, and do so if needed.
    fn maybe_flush(&self, index: &mut FileStoreIndex) -> Result<(), Error> {
        if index.need_flush(self.max_store_size) {
            let locked = self.locked_files.lock().unwrap();
            index
                .flush(self, &locked, self.min_store_size)
                .context("Failed to flush index")?;
        }
        Ok(())
    }
}

impl Drop for FileStore {
    fn drop(&mut self) {
        match self.index.lock() {
            Ok(mut index) => {
                let locked = match self.locked_files.lock() {
                    Ok(l) => l,
                    Err(_) => {
                        warn!("Cannot lock locked_files due to poison");
                        return;
                    }
                };
                if index.need_flush(self.max_store_size) {
                    if let Err(e) = index.flush(self, &locked, self.min_store_size) {
                        warn!("Cannot flush the index: {}", e.to_string());
                    }
                }
                if let Err(e) = index.store(self.base_path.join(STORE_INDEX_FILE)) {
                    warn!("Cannot store the index: {}", e.to_string());
                }
            }
            Err(_) => {
                warn!("Cannot store the index due to poisoned lock");
            }
        }
    }
}

impl FileStoreKey {
    /// Get the suffix of the path of this `FileStoreKey`. For example, if the key is
    /// `aabbccddeeff...` this method will return `aa/bb/aabbccddeeff...`
    fn suffix(&self) -> PathBuf {
        let first = hex::encode([self.hash[0]]);
        let second = hex::encode([self.hash[1]]);
        let full = hex::encode(&self.hash);
        PathBuf::from(first).join(second).join(full)
    }

    /// Make a new `FileStoreKey` from a file on disk. The file must exist and be readable.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FileStoreKey, Error> {
        let path = path.as_ref();
        let mut hasher = Blake2b::new();
        if !path.exists() {
            bail!("Cannot read {}, maybe broken symlink?", path.display())
        }
        let file_reader = ReadFileIterator::new(path)
            .with_context(|| format!("Cannot make file iterator of {}", path.display()))?;
        file_reader.map(|buf| hasher.input(&buf)).last();
        Ok(FileStoreKey {
            hash: hasher.result().to_vec(),
        })
    }

    /// Make a new `FileStoreKey` from an in-memory file.
    pub fn from_content(content: &[u8]) -> FileStoreKey {
        let mut hasher = Blake2b::new();
        hasher.input(content);
        FileStoreKey {
            hash: hasher.result().to_vec(),
        }
    }
}

impl std::fmt::Display for FileStoreKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&hex::encode(&self.hash))
    }
}

impl std::fmt::Debug for FileStoreKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        f.write_str(&self.to_string())
    }
}

impl Serialize for FileStoreKey {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for FileStoreKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let data = String::deserialize(deserializer)?;
        if data.len() < 4 {
            return Err(D::Error::custom("invalid hash"));
        }
        Ok(FileStoreKey {
            hash: hex::decode(data).map_err(|_| D::Error::custom("invalid hash"))?,
        })
    }
}

impl FileStoreHandle {
    /// Make a new handle to a file on disk.
    fn new(store: &FileStore, key: &FileStoreKey) -> FileStoreHandle {
        let path = store.key_to_path(key);
        let mut locked_files = store.locked_files.lock().unwrap();
        *locked_files.ref_counts.entry(key.clone()).or_default() += 1;
        FileStoreHandle {
            path,
            locked_files: store.locked_files.clone(),
            key: key.clone(),
        }
    }

    /// The path to the file pointed by this handle.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The key of the file pointed by this handle.
    pub fn key(&self) -> &FileStoreKey {
        &self.key
    }
}

impl PartialEq for FileStoreHandle {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Clone for FileStoreHandle {
    fn clone(&self) -> Self {
        let mut locked_files = self.locked_files.lock().unwrap();
        *locked_files.ref_counts.entry(self.key.clone()).or_default() += 1;

        FileStoreHandle {
            path: self.path.clone(),
            locked_files: self.locked_files.clone(),
            key: self.key.clone(),
        }
    }
}

impl std::fmt::Display for FileStoreHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.path.display(), f)
    }
}

impl Drop for FileStoreHandle {
    fn drop(&mut self) {
        let mut locked_files = match self.locked_files.lock() {
            Ok(guard) => guard,
            Err(_) => return, // may happen if the thread panicked
        };
        *locked_files
            .ref_counts
            .get_mut(&self.key)
            .expect("Ref counts are broken") -= 1;
        if locked_files.ref_counts[&self.key] == 0 {
            locked_files.ref_counts.remove(&self.key);
        }
    }
}

impl LockedFiles {
    /// Make a new, empty, `LockedFiles`.
    fn new() -> LockedFiles {
        LockedFiles {
            ref_counts: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::*;
    use std::io::{Read, Write};

    use pretty_assertions::{assert_eq, assert_ne};
    use tempdir::TempDir;

    use super::*;

    fn get_cwd() -> TempDir {
        TempDir::new("tm-test").unwrap()
    }

    fn fake_file<P: AsRef<Path>>(path: P, content: &str) -> FileStoreKey {
        File::create(path.as_ref())
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();
        FileStoreKey::from_file(path.as_ref()).unwrap()
    }

    fn add_file_to_store(path: &Path, content: &str, store: &FileStore) -> FileStoreHandle {
        let key = fake_file(path, content);
        let iter = ReadFileIterator::new(path).unwrap();
        store.store(&key, iter).unwrap()
    }

    fn corrupt_file(path: &Path) {
        {
            let file = File::open(&path).unwrap();
            let mut perm = file.metadata().unwrap().permissions();
            perm.set_readonly(false);
            file.set_permissions(perm).unwrap();
        }
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .write_all(b"lol")
            .unwrap();
    }

    #[test]
    fn test_new_filestore() {
        let cwd = get_cwd();
        let _store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        assert!(cwd.path().join(STORE_LOCK_FILE).exists());
    }

    #[test]
    fn test_new_filestore_concurrent() {
        use std::time::*;

        let cwd = get_cwd();
        let store_dir = cwd.path().to_owned();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let thr = std::thread::spawn(move || {
            let start = Instant::now();
            let _store = FileStore::new(&store_dir, 1000, 1000).unwrap();
            let end = Instant::now();
            assert!(end - start >= Duration::from_millis(300));
        });
        std::thread::sleep(Duration::from_millis(500));
        drop(store);
        thr.join().unwrap();
    }

    #[test]
    fn test_store() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "test", &store);
        let path_in_store = store.key_to_path(&handle.key);
        assert!(path_in_store.exists());
        let mut content = String::new();
        File::open(&path_in_store)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(&content, "test");
        assert!(File::open(&path_in_store)
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .readonly());
    }

    #[test]
    fn test_get() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciao", &store);

        let handle = store.get(&handle.key).unwrap();
        let path = handle.path();
        let path_in_store = store.key_to_path(&handle.key);
        assert_eq!(path_in_store, path);
    }

    #[test]
    fn test_get_removed() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciao", &store);
        let path_in_store = store.key_to_path(&handle.key);

        remove_file(path_in_store).unwrap();

        let handle = store.get(&handle.key);
        assert!(handle.is_none());
    }

    #[test]
    fn test_get_not_known() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let key = fake_file(&cwd.path().join("test.txt"), "ciao");
        let handle = store.get(&key);
        assert!(handle.is_none());
    }

    #[test]
    fn test_corrupted_file() {
        if !INTEGRITY_CHECKS_ENABLED {
            return;
        }
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciao", &store);
        let path_in_store = store.key_to_path(&handle.key);
        corrupt_file(&path_in_store);
        let handle = store.get(&handle.key);
        assert!(handle.is_none());
    }

    #[test]
    fn test_key_to_path() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 1000, 1000).unwrap();
        let key = fake_file(&cwd.path().join("test.txt"), "ciao");
        let path = store.key_to_path(&key);
        assert!(path.starts_with(&store.base_path));
        assert!(path.ends_with(key.to_string()));
    }

    #[test]
    fn test_mark_readonly() {
        let cwd = get_cwd();
        let path = cwd.path().join("test.txt");
        File::create(&path).unwrap();
        FileStore::mark_readonly(&path).unwrap();
        assert!(File::open(&path)
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .readonly());
    }

    #[test]
    fn test_remove_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("test.txt");
        File::create(&path).unwrap();
        FileStore::mark_readonly(&path).unwrap();
        FileStore::remove_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_check_integrity() {
        if std::env::var("GITHUB_WORKFLOW").is_ok() || std::env::var("CI").is_ok() {
            // skip this test CI because the runner does not support the last modified time, so the
            // fast integrity check skips the actual sanity check.
            return;
        }
        let cwd = get_cwd();
        let store = FileStore::new(&cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciaone", &store);
        let path = store.key_to_path(&handle.key);
        corrupt_file(&path);
        assert!(!store.check_integrity(&handle.key));
    }

    #[test]
    fn test_locked_files() {
        let cwd = get_cwd();
        let store = FileStore::new(&cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciaone", &store);
        let key = handle.key.clone();
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 1);
        let handle2 = handle.clone();
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 2);
        drop(handle);
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 1);
        drop(handle2);
        assert!(!store
            .locked_files
            .lock()
            .unwrap()
            .ref_counts
            .contains_key(&key));
    }

    #[test]
    fn test_locked_files_different_means() {
        let cwd = get_cwd();
        let store = FileStore::new(&cwd.path(), 1000, 1000).unwrap();
        let handle = add_file_to_store(&cwd.path().join("test.txt"), "ciaone", &store);
        let key = handle.key.clone();
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 1);
        let handle2 = handle.clone();
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 2);
        let handle3 = store.get(&key).unwrap();
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 3);
        drop(handle);
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 2);
        drop(handle3);
        assert_eq!(store.locked_files.lock().unwrap().ref_counts[&key], 1);
        drop(handle2);
        assert!(!store
            .locked_files
            .lock()
            .unwrap()
            .ref_counts
            .contains_key(&key));
    }

    #[test]
    fn test_file_store_key_from_file() {
        let cwd = get_cwd();
        fake_file(&cwd.path().join("file1a.txt"), "ciao");
        fake_file(&cwd.path().join("file1b.txt"), "ciao");
        fake_file(&cwd.path().join("file2.txt"), "ciaone");

        let key1a = FileStoreKey::from_file(&cwd.path().join("file1a.txt")).unwrap();
        let key1b = FileStoreKey::from_file(&cwd.path().join("file1b.txt")).unwrap();
        let key2 = FileStoreKey::from_file(&cwd.path().join("file2.txt")).unwrap();

        assert_eq!(key1a, key1b);
        assert_ne!(key1a, key2);
        assert_ne!(key1b, key2);
    }
}
