mod client;
mod executor;
mod file_transmission;
mod local_executor;
mod scheduler;
mod worker;

use failure::Error;
use std::sync::mpsc::{Receiver, Sender};

pub use client::*;
pub use executor::*;
pub use file_transmission::*;
pub use local_executor::*;
pub use worker::*;

/// The channel part that sends data.
pub type ChannelSender = Sender<String>;
/// The channel part that receives data.
pub type ChannelReceiver = Receiver<String>;

/// Serialize a message into the sender serializing it.
pub fn serialize_into<T>(what: &T, sender: &ChannelSender) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender
        .send(serde_json::to_string(what)?)
        .map_err(|e| e.into())
}

/// Deserialize a message from the channel and return it.
pub fn deserialize_from<T>(reader: &ChannelReceiver) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    serde_json::from_str(&data).map_err(|e| e.into())
}
