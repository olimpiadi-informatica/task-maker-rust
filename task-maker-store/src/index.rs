use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap};
use std::fs::{create_dir_all, remove_dir, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::SystemTime;

use anyhow::{bail, Context, Error};
use serde::{Deserialize, Serialize};

use crate::{FileStore, FileStoreKey, LockedFiles};

/// Magic string that is prepended to the index file to avoid accidental loading of invalid index
/// files.
const MAGIC: &[u8] = b"task-maker-cache";
/// Current version of task-maker, to avoid any problem with serialization/deserialization, changing
/// version will cause a complete index invalidation. Therefore any breaking change to the index
/// file format has to go through a version update.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Maximum number of characters of the version string.
const VERSION_MAX_LEN: usize = 16;

/// An entry of a file inside the file store.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct FileStoreIndexItem {
    /// Size of the file.
    size: u64,
    /// Time of the last read/write of this file.
    last_access: SystemTime,
}

impl Ord for FileStoreIndexItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.last_access.cmp(&other.last_access)
    }
}
impl PartialOrd for FileStoreIndexItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Index with all the files known, allowing efficient LRU file flushing.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FileStoreIndex {
    /// The sum of the size of all the files in the index.
    total_size: u64,
    /// The list of all the files known in the index.
    known_files: HashMap<FileStoreKey, FileStoreIndexItem>,
}

impl FileStoreIndex {
    /// Load the index from the provided path.
    pub(crate) fn load<P: AsRef<Path>>(path: P) -> Result<FileStoreIndex, Error> {
        let path = path.as_ref();
        if !path.exists() {
            debug!("Index at {:?} not found, creating new one", path);
            return Ok(FileStoreIndex {
                total_size: 0,
                known_files: HashMap::new(),
            });
        }

        debug!("Loading index from {:?}", path);
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open index file from {}", path.display()))?;

        let mut magic = [0u8; MAGIC.len() + VERSION_MAX_LEN];
        file.read_exact(&mut magic)
            .context("Failed to read magic number")?;
        if &magic[..MAGIC.len()] != MAGIC {
            bail!(
                "Cache magic mismatch:\nExpected: {:?}\nFound: {:?}",
                MAGIC,
                &magic[..MAGIC.len()]
            );
        }
        if &magic[MAGIC.len()..MAGIC.len() + VERSION.len()] != VERSION.as_bytes() {
            bail!(
                "Cache version mismatch:\nExpected: {:?}\nFound: {:?}",
                VERSION.as_bytes(),
                &magic[MAGIC.len()..MAGIC.len() + VERSION.len()]
            );
        }

        bincode::deserialize_from(file).context("Failed to deserialize index file")
    }

    /// Store a dump of this index to the path provided.
    pub(crate) fn store<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let path = path.as_ref();
        debug!("Saving index file at {}", path.display());

        create_dir_all(path.parent().expect("Invalid store file path"))
            .context("Failed to create store directory")?;
        let tmp = path.with_extension("tmp");

        let mut file = File::create(&tmp)
            .with_context(|| format!("Failed to create index file at {}", tmp.display()))?;
        let mut magic = [0u8; MAGIC.len() + VERSION_MAX_LEN];
        magic[..MAGIC.len()].clone_from_slice(MAGIC);
        magic[MAGIC.len()..MAGIC.len() + VERSION.as_bytes().len()]
            .clone_from_slice(VERSION.as_bytes());

        file.write_all(&magic)
            .context("Failed to write cache magic number")?;

        bincode::serialize_into(file, &self).context("Failed to write index")?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("Failed to move {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    /// Mark a file as accessed, bumping its position in the LRU.
    pub(crate) fn touch(&mut self, key: &FileStoreKey) {
        if let Some(file) = self.known_files.get_mut(key) {
            file.last_access = SystemTime::now();
        }
    }

    /// Add a file in the index if not already present.
    pub(crate) fn add<P: AsRef<Path>>(&mut self, key: FileStoreKey, path: P) -> Result<(), Error> {
        let path = path.as_ref();
        match self.known_files.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().last_access = SystemTime::now();
            }
            Entry::Vacant(entry) => {
                let metadata = std::fs::metadata(path)
                    .with_context(|| format!("Cannot get file metadata of {}", path.display()))?;
                entry.insert(FileStoreIndexItem {
                    size: metadata.len(),
                    last_access: SystemTime::now(),
                });
                self.total_size += metadata.len();
            }
        }
        Ok(())
    }

    /// Whether this file store needs to flush away some files to free space.
    pub(crate) fn need_flush(&self, size_limit: u64) -> bool {
        self.total_size >= size_limit
    }

    /// Perform a flushing operation, cleaning some space on the disk by removing the Least Recently
    /// Used files. This function won't remove the files currently locked.
    pub(crate) fn flush(
        &mut self,
        file_store: &FileStore,
        locked_files: &LockedFiles,
        target_size: u64,
    ) -> Result<(), Error> {
        debug!(
            "Starting flushing process from {}MiB to at most {}MiB",
            self.total_size / 1024 / 1024,
            target_size / 1024 / 1024
        );
        // list of entries that survive the flush
        let mut surviving = Vec::new();
        let mut priority_queue: BinaryHeap<(FileStoreIndexItem, FileStoreKey)> =
            self.known_files.drain().map(|(k, f)| (f, k)).collect();
        // number of removed bytes
        let mut removed = 0;
        // continue to remove until the space requirement is met
        while self.total_size > target_size {
            let (entry, key) = match priority_queue.pop() {
                Some(e) => e,
                // the queue is emptied before reaching the space requirement (maybe because of
                // locking)
                None => break,
            };
            // cannot remove a file used by some other process
            if locked_files.ref_counts.contains_key(&key) {
                surviving.push((key, entry));
            } else {
                self.total_size -= entry.size;
                removed += entry.size;

                let path = file_store.key_to_path(&key);
                debug!("Removing file {:?} claiming {}KiB", path, entry.size / 1024);
                if let Err(e) = FileStore::remove_file(&path) {
                    warn!("Cannot flush file {:?}: {}", path, e.to_string());
                }
                let base_path = file_store.base_path.canonicalize().with_context(|| {
                    format!(
                        "Invalid file store base path: {}",
                        file_store.base_path.display()
                    )
                })?;
                let mut path = path.parent();
                // remove empty directories until the root of the store is reached
                while let Some(p) = path {
                    if p == base_path {
                        break;
                    }
                    debug!("Removing {:?}", p);
                    if remove_dir(p).is_err() {
                        debug!("... it wasn't empty");
                        break;
                    }
                    path = p.parent();
                }
            }
        }
        debug!("Claimed {}KiB", removed / 1024);
        // the locked files that have been removed from the queue
        for (key, entry) in surviving {
            self.known_files.insert(key, entry);
        }
        // the files that survived the flush because are at new enough
        for (entry, key) in priority_queue {
            self.known_files.insert(key, entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    use pretty_assertions::{assert_eq, assert_ne};
    use std::time::Duration;
    use tempdir::TempDir;

    use crate::{FileStore, FileStoreHandle, FileStoreKey, ReadFileIterator};

    fn get_cwd() -> TempDir {
        TempDir::new("tm-test").unwrap()
    }

    fn fake_file<P: AsRef<Path>>(path: P, content: u8, len: usize) -> FileStoreKey {
        let data = vec![content; len];
        File::create(path.as_ref())
            .unwrap()
            .write_all(&data)
            .unwrap();
        FileStoreKey::from_file(path.as_ref()).unwrap()
    }

    fn add_file_to_store(store: &FileStore, len: usize) -> FileStoreHandle {
        let path = store.base_path.join("temp.txt");
        let key = fake_file(&path, 123, len);
        let iter = ReadFileIterator::new(path).unwrap();
        store.store(&key, iter).unwrap()
    }

    #[test]
    fn test_empty_index() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        assert_eq!(store.max_store_size, 200);
        assert_eq!(store.min_store_size, 100);
        let index = store.index.lock().unwrap();
        assert_eq!(index.total_size, 0);
        assert_eq!(index.known_files.len(), 0);
    }

    #[test]
    fn test_load_index() {
        let cwd = get_cwd();
        {
            let store = FileStore::new(cwd.path(), 200, 100).unwrap();
            add_file_to_store(&store, 50);
            let index = store.index.lock().unwrap();
            assert_eq!(index.total_size, 50);
            assert_eq!(index.known_files.len(), 1);
            // store index on drop
        }
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        let index = store.index.lock().unwrap();
        assert_eq!(index.total_size, 50);
        assert_eq!(index.known_files.len(), 1);
    }

    #[test]
    fn test_no_flush() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        add_file_to_store(&store, 10);
        add_file_to_store(&store, 20);
        add_file_to_store(&store, 30);
        store.maybe_flush(&mut store.index.lock().unwrap()).unwrap();
        let index = store.index.lock().unwrap();
        assert_eq!(index.total_size, 60);
        assert_eq!(index.known_files.len(), 3);
    }

    #[test]
    fn test_no_duplicates() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        add_file_to_store(&store, 10);
        add_file_to_store(&store, 20);
        add_file_to_store(&store, 20);
        store.maybe_flush(&mut store.index.lock().unwrap()).unwrap();
        let index = store.index.lock().unwrap();
        assert_eq!(index.total_size, 30);
        assert_eq!(index.known_files.len(), 2);
    }

    #[test]
    fn test_flush() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        let key1 = add_file_to_store(&store, 90).key.clone();
        let key2 = add_file_to_store(&store, 95).key.clone();
        store.maybe_flush(&mut store.index.lock().unwrap()).unwrap();
        assert_eq!(store.index.lock().unwrap().total_size, 185);
        let key3 = add_file_to_store(&store, 50).key.clone();
        store.maybe_flush(&mut store.index.lock().unwrap()).unwrap();
        let index = store.index.lock().unwrap();
        assert_eq!(index.total_size, 50);
        assert_eq!(index.known_files.len(), 1);
        assert!(!store.key_to_path(&key1).exists());
        assert!(!store.key_to_path(&key2).exists());
        assert!(store.key_to_path(&key3).exists());
    }

    #[test]
    fn test_flush_locked() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        let handle1 = add_file_to_store(&store, 90);
        let key2 = add_file_to_store(&store, 95).key.clone();
        store.maybe_flush(&mut store.index.lock().unwrap()).unwrap();
        assert_eq!(store.index.lock().unwrap().total_size, 185);
        let key3 = add_file_to_store(&store, 50).key.clone();

        // force flush because the last store did a flush removing the 90
        let mut index = store.index.lock().unwrap();
        let locked = store.locked_files.lock().unwrap();
        index.flush(&store, &locked, 100).unwrap();

        assert_eq!(index.total_size, 90);
        assert_eq!(index.known_files.len(), 1);
        assert!(handle1.path.exists());
        assert!(!store.key_to_path(&key2).exists());
        assert!(!store.key_to_path(&key3).exists());
    }

    #[test]
    fn test_flush_touch() {
        let cwd = get_cwd();
        let store = FileStore::new(cwd.path(), 200, 100).unwrap();
        let handle = add_file_to_store(&store, 10);
        let mut index = store.index.lock().unwrap();
        let before = index.known_files[&handle.key].last_access;
        std::thread::sleep(Duration::from_millis(100));
        let after1 = index.known_files[&handle.key].last_access;
        assert_eq!(before, after1);
        index.touch(&handle.key);
        let after2 = index.known_files[&handle.key].last_access;
        assert_ne!(before, after2);
    }
}
