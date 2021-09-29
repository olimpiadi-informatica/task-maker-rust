use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Error};
use itertools::Itertools;

use crate::entry::CacheEntry;
use crate::key::CacheKey;
use crate::Cache;

/// Magic string that is prepended to the cache file to avoid accidental loading of invalid cache
/// files.
const MAGIC: &[u8] = b"task-maker-cache";
/// Current version of task-maker, to avoid any problem with serialization/deserialization, changing
/// version will cause a complete cache invalidation. Therefore any breaking change to the cache
/// file format has to go through a version update.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Maximum number of characters of the version string.
const VERSION_MAX_LEN: usize = 16;

static_assertions::const_assert!(VERSION.len() <= VERSION_MAX_LEN);

/// Read the cache file, check the magic string and deserialize all the entries in it.
pub fn load<P: AsRef<Path>>(path: P) -> Result<HashMap<CacheKey, Vec<CacheEntry>>, Error> {
    let mut file = std::fs::File::open(&path)
        .with_context(|| format!("Cannot open cache file at {}", path.as_ref().display()))?;
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

    Ok(
        bincode::deserialize_from::<_, Vec<(CacheKey, Vec<CacheEntry>)>>(file)
            .context("Failed to deserialize cache content")?
            .into_iter()
            .collect(),
    )
}

/// Store the content of the cache to the cache file, including the magic string.
pub fn store(cache: &Cache) -> Result<(), Error> {
    std::fs::create_dir_all(cache.cache_file.parent().expect("Invalid cache file"))
        .context("Failed to create cache directory")?;
    let mut file =
        std::fs::File::create(&cache.cache_file).context("Failed to create cache file")?;

    let mut magic = [0u8; MAGIC.len() + VERSION_MAX_LEN];
    magic[..MAGIC.len()].clone_from_slice(MAGIC);
    magic[MAGIC.len()..MAGIC.len() + VERSION.as_bytes().len()].clone_from_slice(VERSION.as_bytes());

    file.write_all(&magic)
        .context("Failed to write cache magic number")?;

    let serialized = bincode::serialize(&cache.entries.iter().collect_vec())
        .context("Failed to serialize cache content")?;
    file.write_all(&serialized)
        .context("Failed to write cache content")?;
    Ok(())
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

        assert!(load(&path).is_err());
    }

    #[test]
    fn test_load_reject_wrong_version() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("cache");
        let mut f = File::create(&path).unwrap();
        f.write_all(MAGIC).unwrap();
        f.write_all(b"wrong-version").unwrap();

        assert!(load(&path).is_err());
    }

    #[test]
    fn test_load_after_store() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("cache");
        let cache = Cache {
            entries: Default::default(),
            cache_file: path.clone(),
        };

        store(&cache).unwrap();
        assert!(load(&path).is_ok());
    }
}
