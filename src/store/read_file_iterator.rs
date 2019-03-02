use failure::Error;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const READ_FILE_BUFFER_SIZE: usize = 1024;
type ReadFileBuffer = [u8; READ_FILE_BUFFER_SIZE];

pub struct ReadFileIterator {
    buf_reader: BufReader<File>,
    buf: ReadFileBuffer,
}

impl ReadFileIterator {
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
    fn next<'a>(&mut self) -> Option<Self::Item> {
        match self.buf_reader.read(&mut self.buf) {
            Ok(0) => None,
            Ok(n) => Some(self.buf[0..n].to_vec()),
            Err(_) => None,
        }
    }
}
