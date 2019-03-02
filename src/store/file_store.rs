use crate::store::*;
use blake2::{Blake2b, Digest};
use chrono::prelude::*;
use failure::{Error, Fail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const PERSISTENCY_DURATION: Duration = Duration::from_secs(600);
const CHECK_INTEGRITY: bool = true;

pub type HashData = Vec<u8>;

pub struct FileStore {
    base_path: String,
    file: File,
    data: FileStoreData,
}

#[derive(Serialize, Deserialize)]
pub struct FileStoreKey {
    hash: HashData,
}

#[derive(Debug, Fail)]
pub enum FileStoreError {
    #[fail(display = "invalid path provided")]
    InvalidPath,
    #[fail(display = "file not present in the store")]
    NotFound,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileStoreItem {
    persistent: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileStoreData {
    items: HashMap<String, FileStoreItem>,
}

impl FileStoreKey {
    pub fn from_file(path: &Path) -> Result<FileStoreKey, Error> {
        let mut hasher = Blake2b::new();
        let file_reader = ReadFileIterator::new(path)?;
        file_reader.map(|buf| hasher.input(&buf)).last();
        Ok(FileStoreKey {
            hash: hasher.result().to_vec(),
        })
    }

    pub fn to_string(&self) -> String {
        hex::encode(&self.hash)
    }
}

impl std::fmt::Debug for FileStoreKey {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        fmt.write_str(&hex::encode(&self.hash))
    }
}

impl FileStoreItem {
    fn new() -> FileStoreItem {
        FileStoreItem {
            persistent: Utc::now(),
        }
    }

    fn persist(&mut self) {
        let now = Utc::now().timestamp();
        let target = now + (PERSISTENCY_DURATION.as_secs() as i64);
        self.persistent = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(target, 0), Utc);
    }
}

impl FileStoreData {
    fn new() -> FileStoreData {
        FileStoreData {
            items: HashMap::new(),
        }
    }

    fn get_mut(&mut self, key: &FileStoreKey) -> &mut FileStoreItem {
        let key = key.to_string();
        if !self.items.contains_key(&key) {
            self.items.insert(key.clone(), FileStoreItem::new());
        }
        self.items.get_mut(&key).unwrap()
    }

    fn remove(&mut self, key: &FileStoreKey) -> Option<FileStoreItem> {
        self.items.remove(&key.to_string())
    }
}

impl FileStore {
    pub fn new(base_path: &Path) -> Result<FileStore, Error> {
        std::fs::create_dir_all(base_path)?;
        let path = match Path::new(base_path).join("store_info").to_str() {
            Some(path) => path.to_owned(),
            None => return Err(FileStoreError::InvalidPath.into()),
        };
        if !Path::new(&path).exists() {
            serde_json::to_writer(File::create(&path)?, &FileStoreData::new())?;
        }
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if let Err(e) = file.try_lock_exclusive() {
            if e.to_string() != fs2::lock_contended_error().to_string() {
                return Err(e.into());
            }
            warn!("Store locked... waiting");
            file.lock_exclusive()?;
        }
        let data = FileStore::read_store_file(&file, base_path)?;
        Ok(FileStore {
            base_path: base_path.to_string_lossy().to_string(),
            file,
            data,
        })
    }

    pub fn store<I>(&mut self, key: &FileStoreKey, content: I) -> Result<(), Error>
    where
        I: Iterator<Item = Vec<u8>>,
    {
        let path = self.key_to_path(key);
        if path.exists() {
            trace!("File {:?} already exists", path);
            content.last(); // consume all the iterator
            self.data.get_mut(key).persist();
            self.flush()?;
            return Ok(());
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut file = std::fs::File::create(&path)?;
        content.map(|data| file.write_all(&data)).last();
        FileStore::mark_readonly(&path)?;
        self.data.get_mut(key).persist();
        self.flush()?;
        Ok(())
    }

    pub fn get(&mut self, key: &FileStoreKey) -> Result<Option<PathBuf>, Error> {
        let path = self.key_to_path(key);
        if !path.exists() {
            self.data.remove(&key);
            self.flush()?;
            return Ok(None);
        }
        if CHECK_INTEGRITY {
            if !self.check_integrity(key) {
                warn!("File {:?} failed the integrity check", path);
                self.data.remove(key);
                FileStore::remove_file(&path)?;
                return Ok(None);
            }
        }
        self.persist(key)?;
        Ok(Some(path))
    }

    pub fn has_key(&self, key: &FileStoreKey) -> bool {
        self.key_to_path(key).exists()
    }

    pub fn persist(&mut self, key: &FileStoreKey) -> Result<(), Error> {
        let path = self.key_to_path(key);
        if !path.exists() {
            return Err(FileStoreError::NotFound.into());
        }
        self.data.get_mut(key).persist();
        self.flush()?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        let serialized = serde_json::to_string(&self.data)?;
        self.file.seek(std::io::SeekFrom::Start(0))?;
        self.file.write_all(serialized.as_bytes())?;
        self.file.set_len(serialized.len() as u64)?;
        Ok(())
    }

    fn key_to_path(&self, key: &FileStoreKey) -> PathBuf {
        let first = hex::encode(vec![key.hash[0]]);
        let second = hex::encode(vec![key.hash[1]]);
        let full = hex::encode(&key.hash);
        Path::new(&self.base_path)
            .join(first)
            .join(second)
            .join(full)
            .to_owned()
    }

    fn read_store_file(file: &File, base_path: &Path) -> Result<FileStoreData, Error> {
        let mut data: FileStoreData = serde_json::from_reader(file)?;
        // remove files not present anymore
        data.items = data
            .items
            .into_iter()
            .filter(|(key, _)| {
                base_path
                    .join(&key[0..2])
                    .join(&key[2..4])
                    .join(key)
                    .exists()
            })
            .collect();
        Ok(data)
    }

    fn mark_readonly(path: &Path) -> Result<(), Error> {
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }

    fn remove_file(path: &Path) -> Result<(), Error> {
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)?;
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn check_integrity(&self, key: &FileStoreKey) -> bool {
        let path = self.key_to_path(key);
        match FileStoreKey::from_file(&path) {
            Ok(key2) => key2.hash == key.hash,
            Err(_) => false,
        }
    }
}
