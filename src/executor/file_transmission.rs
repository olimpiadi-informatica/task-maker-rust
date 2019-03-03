use crate::executor::*;
use crate::store::ReadFileIterator;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Serialize, Deserialize)]
pub enum FileProtocol {
    Data(Vec<u8>),
    End,
}

pub struct ChannelFileIterator<'a> {
    reader: &'a Receiver<String>,
}

impl<'a> ChannelFileIterator<'a> {
    pub fn new(reader: &'a Receiver<String>) -> ChannelFileIterator<'a> {
        ChannelFileIterator { reader }
    }
}

impl<'a> Iterator for ChannelFileIterator<'a> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Self::Item> {
        match deserialize_from::<FileProtocol>(self.reader).unwrap() {
            FileProtocol::Data(d) => Some(d),
            FileProtocol::End => None,
        }
    }
}

pub struct ChannelFileSender;

impl ChannelFileSender {
    pub fn send(path: &Path, sender: &Sender<String>) -> Result<(), Error> {
        for buf in ReadFileIterator::new(path)? {
            serialize_into(&FileProtocol::Data(buf), sender)?;
        }
        serialize_into(&FileProtocol::End, sender)?;
        Ok(())
    }
}
