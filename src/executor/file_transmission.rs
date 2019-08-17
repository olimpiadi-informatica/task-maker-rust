use crate::executor::*;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::Path;
use task_maker_store::ReadFileIterator;

/// Messages sent during the FileProtocol operation, ie during the transfer
/// of a file
#[derive(Debug, Serialize, Deserialize)]
pub enum FileProtocol {
    /// A chunk of data
    Data(Vec<u8>),
    /// The end of the stream
    End,
}

/// An iterator over the byte chunks sent during the FileProtocol mode in a
/// channel
pub struct ChannelFileIterator<'a> {
    /// Reference to the channel from where to read
    reader: &'a ChannelReceiver,
}

impl<'a> ChannelFileIterator<'a> {
    /// Create a new iterator over a receiver channel
    pub fn new(reader: &'a ChannelReceiver) -> ChannelFileIterator<'a> {
        ChannelFileIterator { reader }
    }
}

impl<'a> Iterator for ChannelFileIterator<'a> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        // errors cannot be handled in this iterator yet
        match deserialize_from::<FileProtocol>(self.reader).unwrap() {
            FileProtocol::Data(d) => Some(d),
            FileProtocol::End => None,
        }
    }
}

/// Utility function to send a file to a channel using FileProtocol
pub struct ChannelFileSender;

impl ChannelFileSender {
    /// Send a local file to a channel using FileProtocol
    pub fn send(path: &Path, sender: &ChannelSender) -> Result<(), Error> {
        for buf in ReadFileIterator::new(path)? {
            serialize_into(&FileProtocol::Data(buf), sender)?;
        }
        serialize_into(&FileProtocol::End, sender)?;
        Ok(())
    }
}
