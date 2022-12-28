use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Error};
use const_format::formatcp;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::entry::CacheEntry;
use crate::key::CacheKey;

/// Magic string that is prepended to the cache file to avoid accidental loading of invalid cache
/// files.
///
/// The newline at the end of the string is required. For example, let's say there are 2 versions:
/// v0.1 and v0.11; running v0.11 first, and then v0.1, without the newline the magic of the old
/// version is a prefix of the magic of the new version.
const MAGIC: &[u8] = formatcp!("task-maker-cache v{}\n", env!("CARGO_PKG_VERSION")).as_bytes();

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

        let file = std::fs::File::open(&path)
            .with_context(|| format!("Cannot open cache file at {}", path.display()))?;
        let mut reader = BufReader::new(file);
        let mut magic = [0u8; MAGIC.len()];

        if reader
            .read_exact(&mut magic)
            .map_or(false, |_| magic != MAGIC)
        {
            info!(
                "Cache version mismatch:\nExpected: {:?}\nFound: {:?}",
                MAGIC, magic
            );
            return Ok(Self {
                entries: Default::default(),
                path,
                dirty: false,
            });
        }

        let entries = bincode::deserialize_from::<_, HashMap<CacheKey, Vec<CacheEntry>>>(reader)
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
        let file = std::fs::File::create(&tmp).context("Failed to create cache file")?;
        let mut writer = BufWriter::new(file);

        writer
            .write_all(MAGIC)
            .context("Failed to write cache magic number")?;

        bincode::serialize_into(writer, &self.entries.iter().collect_vec())
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
        let tmpdir = tempfile::TempDir::new().unwrap();
        let path = tmpdir.path().join("cache");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"totally-not-the-magic").unwrap();

        assert!(CacheFile::load(path).is_err());
    }

    #[test]
    fn test_load_reject_wrong_version() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let path = tmpdir.path().join("cache");
        let mut f = File::create(&path).unwrap();
        f.write_all(MAGIC).unwrap();
        f.write_all(b"wrong-version").unwrap();

        assert!(CacheFile::load(path).is_err());
    }
}
