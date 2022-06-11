use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{bail, Context, Error};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::entry::CacheEntry;
use crate::key::CacheKey;

/// Magic string that is prepended to the cache file to avoid accidental loading of invalid cache
/// files.
const MAGIC: &[u8] = b"task-maker-cache";
/// Current version of task-maker, to avoid any problem with serialization/deserialization, changing
/// version will cause a complete cache invalidation. Therefore any breaking change to the cache
/// file format has to go through a version update.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Maximum number of characters of the version string.
const VERSION_MAX_LEN: usize = 16;

/// A cache file.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CacheFile {
    /// The set of entries in this cache file.
    entries: HashMap<CacheKey, Vec<CacheEntry>>,
    /// Where this file is stored.
    path: PathBuf,
    /// Whether this file should be flushed.
    dirty: bool,
}

static_assertions::const_assert!(VERSION.len() <= VERSION_MAX_LEN);

impl CacheFile {
    /// Read the cache file, check the magic string and deserialize all the entries in it.
    pub fn load(path: PathBuf) -> Result<CacheFile, Error> {
        if !path.exists() {
            return Ok(Self {
                entries: Default::default(),
                path,
                dirty: false,
            });
        }
        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("Cannot open cache file at {}", path.display()))?;
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

        let entries = bincode::deserialize_from::<_, HashMap<CacheKey, Vec<CacheEntry>>>(file)
            .context("Failed to deserialize cache content")?;

        Ok(Self {
            entries,
            path,
            dirty: false,
        })
    }

    /// Store the content of the cache to the cache file, including the magic string.
    pub fn store(&self) -> Result<(), Error> {
        // Do not write the file if it's not dirty.
        if !self.dirty {
            return Ok(());
        }

        let path = &self.path;
        std::fs::create_dir_all(path.parent().context("Invalid cache file")?)
            .with_context(|| format!("Failed to create cache directory for {}", path.display()))?;
        let tmp = path.with_extension("tmp");
        let mut file = std::fs::File::create(&tmp).context("Failed to create cache file")?;

        let mut magic = [0u8; MAGIC.len() + VERSION_MAX_LEN];
        magic[..MAGIC.len()].clone_from_slice(MAGIC);
        magic[MAGIC.len()..MAGIC.len() + VERSION.as_bytes().len()]
            .clone_from_slice(VERSION.as_bytes());

        file.write_all(&magic)
            .context("Failed to write cache magic number")?;

        bincode::serialize_into(file, &self.entries.iter().collect_vec())
            .context("Failed to write cache content")?;
        std::fs::rename(&tmp, &self.path).with_context(|| {
            format!(
                "Failed to move {} -> {}",
                tmp.display(),
                self.path.display()
            )
        })?;
        Ok(())
    }

    pub fn entry(&mut self, key: CacheKey) -> Entry<CacheKey, Vec<CacheEntry>> {
        self.entries.entry(key)
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_load_reject_wrong_magic() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("cache");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"totally-not-the-magic").unwrap();

        assert!(CacheFile::load(path).is_err());
    }

    #[test]
    fn test_load_reject_wrong_version() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("cache");
        let mut f = File::create(&path).unwrap();
        f.write_all(MAGIC).unwrap();
        f.write_all(b"wrong-version").unwrap();

        assert!(CacheFile::load(path).is_err());
    }
}
