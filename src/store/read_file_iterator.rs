use failure::Error;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Buffer size when reading a file
const READ_FILE_BUFFER_SIZE: usize = 8 * 1024;
/// Type of the reading buffer
type ReadFileBuffer = [u8; READ_FILE_BUFFER_SIZE];

/// Struct implementing the Iterator trait which will iterate over the content
/// of a file
pub struct ReadFileIterator {
    /// Reader used to read the file
    buf_reader: BufReader<File>,
    /// Current read buffer
    buf: ReadFileBuffer,
}

impl ReadFileIterator {
    /// Make a new iterator reading the file at that path
    pub fn new(path: &Path) -> Result<ReadFileIterator, Error> {
        let file = std::fs::File::open(path)?;
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
