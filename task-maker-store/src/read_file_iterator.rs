use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Error};

/// Buffer size when reading a file
const READ_FILE_BUFFER_SIZE: usize = 8 * 1024;
/// Type of the reading buffer
type ReadFileBuffer = [u8; READ_FILE_BUFFER_SIZE];

/// Struct implementing the Iterator trait which will iterate over the content
/// of a file.
///
/// # Example
///
/// ```
/// use task_maker_store::ReadFileIterator;
/// # use tempfile::TempDir;
///
/// # use anyhow::Error;
/// # fn main() -> Result<(), Error> {
/// # let tmp = TempDir::new().unwrap();
/// # let path = tmp.path().join("file.txt");
/// std::fs::write(&path, "hello world")?;
/// let iter = ReadFileIterator::new(&path)?;
/// let content: Vec<u8> = iter.flat_map(|v| v).collect();
/// println!("The content is {}", std::str::from_utf8(&content[..])?);
/// # Ok(())
/// # }
/// ```
pub struct ReadFileIterator {
    /// Reader used to read the file
    buf_reader: BufReader<File>,
    /// Current read buffer
    buf: ReadFileBuffer,
}

impl ReadFileIterator {
    /// Make a new iterator reading the file at that path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<ReadFileIterator, Error> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        Ok(ReadFileIterator {
            buf_reader: BufReader::new(file),
            buf: [0; READ_FILE_BUFFER_SIZE],
        })
    }
}

impl Iterator for ReadFileIterator {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.buf_reader.read(&mut self.buf) {
            Ok(0) => None,
            Ok(n) => Some(self.buf[0..n].to_vec()),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn get_cwd() -> TempDir {
        TempDir::new().unwrap()
    }

    fn fake_file(path: &Path, content: Vec<u8>) {
        File::create(path).unwrap().write_all(&content).unwrap();
    }

    #[test]
    fn test_read_file_iterator_404() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        let iter = ReadFileIterator::new(path);
        assert!(iter.is_err());
    }

    #[test]
    fn test_read_file_iterator_empty_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        fake_file(&path, vec![]);
        let mut iter = ReadFileIterator::new(&path).unwrap();
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_read_file_iterator_small_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        fake_file(&path, vec![1, 2, 3, 4]);
        let mut iter = ReadFileIterator::new(&path).unwrap();
        let chunk = iter.next();
        assert_eq!(chunk, Some(vec![1, 2, 3, 4]));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_read_file_iterator_chunk_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        let content = vec![123; READ_FILE_BUFFER_SIZE];
        fake_file(&path, content.clone());
        let mut iter = ReadFileIterator::new(&path).unwrap();
        let chunk = iter.next();
        assert_eq!(chunk, Some(content));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_read_file_iterator_chunk_and_a_half_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        let content = vec![123; READ_FILE_BUFFER_SIZE + 1];
        fake_file(&path, content.clone());
        let mut iter = ReadFileIterator::new(&path).unwrap();
        assert_eq!(
            iter.next(),
            Some(content[0..READ_FILE_BUFFER_SIZE].to_owned())
        );
        assert_eq!(iter.next(), Some(vec![123]));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_read_file_iterator_many_chunks_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        let num_chunks = 3;
        let content = vec![123; READ_FILE_BUFFER_SIZE * num_chunks];
        fake_file(&path, content);
        let mut iter = ReadFileIterator::new(&path).unwrap();
        for _ in 0..num_chunks {
            let chunk = iter.next().unwrap();
            assert_eq!(chunk.len(), READ_FILE_BUFFER_SIZE);
            assert!(chunk.iter().all(|c| c == &123));
        }
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_read_file_iterator_random_file() {
        let cwd = get_cwd();
        let path = cwd.path().join("file.txt");
        let mut content = Vec::new();
        let num_chunks = 3;
        let change = 123;
        for i in 0..READ_FILE_BUFFER_SIZE * num_chunks + change {
            content.push(i as u8);
        }
        fake_file(&path, content.clone());
        let mut iter = ReadFileIterator::new(&path).unwrap();
        let mut pos = 0;
        for _ in 0..num_chunks {
            let chunk = iter.next().unwrap();
            assert_eq!(chunk.len(), READ_FILE_BUFFER_SIZE);
            for b in chunk {
                assert_eq!(b, content[pos]);
                pos += 1;
            }
        }
        let chunk = iter.next().unwrap();
        for b in chunk {
            assert_eq!(b, content[pos]);
            pos += 1;
        }
        assert_eq!(pos, content.len());
        assert_eq!(iter.next(), None);
    }
}
