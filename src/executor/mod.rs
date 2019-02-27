mod client;
mod executor;
mod local_executor;
mod scheduler;
mod worker;

use failure::Error;
use std::sync::mpsc::{Receiver, Sender};

pub use client::*;
pub use executor::*;
pub use local_executor::*;
pub use worker::*;

fn serialize_into<T>(what: &T, sender: &Sender<String>) -> Result<(), Error>
where
    T: serde::Serialize,
{
    sender
        .send(serde_json::to_string(what)?)
        .map_err(|e| e.into())
}

fn deserialize_from<T>(reader: &Receiver<String>) -> Result<T, Error>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let data = reader.recv()?;
    serde_json::from_str(&data).map_err(|e| e.into())
}
