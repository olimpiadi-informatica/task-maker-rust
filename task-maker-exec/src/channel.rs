use std::cell::RefCell;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};
use failure::{bail, Error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::proto::FileProtocol;

/// Message type that can be send in a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelMessage<T> {
    /// The message is a normal application message of type T.
    Message(T),
    /// The message encodes a `FileProtocol` message. This variant is only used in local channels.
    FileProtocol(FileProtocol),
    /// Message telling the other end that a file is coming of the specified length. This variant is
    /// only used in remote channels.
    RawFileData(usize),
    /// Message telling the other end that the file is ended, i.e. this was the last chunk. This
    /// variant is only used in remote channels.
    RawFileEnd,
}

/// The channel part that sends data.
#[derive(Debug, Clone)]
pub enum ChannelSender<T> {
    /// The connection is only a local in-memory channel.
    Local(Sender<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(Arc<Mutex<TcpStream>>),
}

/// The channel part that receives data.
#[derive(Debug)]
pub enum ChannelReceiver<T> {
    /// The connection is only a local in-memory channel.
    Local(Receiver<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(RefCell<TcpStream>),
}

impl<T> ChannelSender<T>
where
    T: 'static + Send + Sync + Serialize,
{
    /// Send some data into the channel.
    pub fn send(&self, data: T) -> Result<(), Error> {
        match self {
            ChannelSender::Local(sender) => sender
                .send(ChannelMessage::Message(data))
                .map_err(|e| e.into()),
            ChannelSender::Remote(sender) => {
                ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::Message(data))
            }
        }
    }

    /// Send some `FileProtocol` data in the channel.
    pub(crate) fn send_file(&self, data: FileProtocol) -> Result<(), Error> {
        match self {
            ChannelSender::Local(sender) => Ok(sender.send(ChannelMessage::FileProtocol(data))?),
            ChannelSender::Remote(sender) => match data {
                // Data is special, to avoid costly serialization of raw bytes, send the size of the
                // buffer and then the raw content.
                FileProtocol::Data(data) => {
                    ChannelSender::<T>::send_remote_raw(
                        sender,
                        ChannelMessage::RawFileData(data.len()),
                    )?;
                    let mut sender = sender.lock().expect("Cannot lock ChannelSender");
                    let stream = sender.deref_mut();
                    stream.write_all(&data).map_err(|e| e.into())
                }
                FileProtocol::End => {
                    ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::RawFileEnd)
                }
            },
        }
    }

    /// Send some unchecked data to the remote channel.
    fn send_remote_raw(
        sender: &Arc<Mutex<TcpStream>>,
        data: ChannelMessage<T>,
    ) -> Result<(), Error> {
        let mut sender = sender.lock().expect("Cannot lock ChannelSender");
        let stream = sender.deref_mut();
        bincode::serialize_into(stream, &data)?;
        Ok(())
    }

    /// Given this is a `ChannelSender::Remote`, change the type of the message. Will panic if this
    /// is a `ChannelSender::Local`.
    ///
    /// This function is useful for implementing a protocol where the message types change during
    /// the execution, for example because initially there is an handshake message, followed by the
    /// actual protocol messages.
    pub fn change_type<T2>(self) -> ChannelSender<T2> {
        match self {
            ChannelSender::Local(_) => panic!("Cannot change ChannelSender::Local type"),
            ChannelSender::Remote(r) => ChannelSender::Remote(r),
        }
    }
}

impl<'a, T> ChannelReceiver<T>
where
    T: 'static + DeserializeOwned,
{
    /// Receive a message from the channel.
    pub fn recv(&self) -> Result<T, Error> {
        let message = match self {
            ChannelReceiver::Local(receiver) => receiver.recv()?,
            ChannelReceiver::Remote(receiver) => ChannelReceiver::recv_remote_raw(receiver)?,
        };
        match message {
            ChannelMessage::Message(mex) => Ok(mex),
            _ => bail!("Expected ChannelMessage::Message"),
        }
    }

    /// Receive a message of the `FileProtocol` from the channel.
    pub(crate) fn recv_file(&self) -> Result<FileProtocol, Error> {
        match self {
            ChannelReceiver::Local(receiver) => match receiver.recv()? {
                ChannelMessage::FileProtocol(data) => Ok(data),
                _ => bail!("Expected ChannelMessage::FileProtocol"),
            },
            ChannelReceiver::Remote(receiver) => {
                match ChannelReceiver::<T>::recv_remote_raw(receiver)? {
                    ChannelMessage::RawFileData(len) => {
                        let mut receiver = receiver.borrow_mut();
                        let mut buf = vec![0u8; len];
                        receiver.read_exact(&mut buf)?;
                        Ok(FileProtocol::Data(buf))
                    }
                    ChannelMessage::RawFileEnd => Ok(FileProtocol::End),
                    _ => {
                        bail!("Expected ChannelMessage::RawFileData or ChannelMessage::RawFileEnd")
                    }
                }
            }
        }
    }

    /// Receive a message from the TCP stream of a channel.
    fn recv_remote_raw(receiver: &RefCell<TcpStream>) -> Result<ChannelMessage<T>, Error> {
        let mut receiver = receiver.borrow_mut();
        Ok(bincode::deserialize_from(receiver.deref_mut())?)
    }

    /// Given this is a `ChannelReceiver::Remote`, change the type of the message. Will panic if
    /// this is a `ChannelReceiver::Local`.
    ///
    /// This function is useful for implementing a protocol where the message types change during
    /// the execution, for example because initially there is an handshake message, followed by the
    /// actual protocol messages.
    pub fn change_type<T2>(self) -> ChannelReceiver<T2> {
        match self {
            ChannelReceiver::Local(_) => panic!("Cannot change ChannelReceiver::Local type"),
            ChannelReceiver::Remote(r) => ChannelReceiver::Remote(r),
        }
    }
}

/// Make a new pair of `ChannelSender` / `ChannelReceiver`
pub fn new_local_channel<T>() -> (ChannelSender<T>, ChannelReceiver<T>) {
    let (tx, rx) = unbounded();
    (ChannelSender::Local(tx), ChannelReceiver::Local(rx))
}

/// Listener for connections on some TCP socket.
///
/// `S` and `R` are the types of message sent and received respectively.
pub struct ChannelServer<S, R>(TcpListener, PhantomData<*const S>, PhantomData<*const R>);

impl<S, R> ChannelServer<S, R> {
    /// Bind a socket and create a new `ChannelServer`.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer(
            TcpListener::bind(addr)?,
            PhantomData::default(),
            PhantomData::default(),
        ))
    }
}

impl<S, R> Deref for ChannelServer<S, R> {
    type Target = TcpListener;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, R> Iterator for ChannelServer<S, R> {
    type Item = (ChannelSender<S>, ChannelReceiver<R>, SocketAddr);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self
                .0
                .incoming()
                .next()
                .expect("TcpListener::incoming returned None");
            if let Ok(s) = next {
                let peer_addr = s.peer_addr().expect("Peer has no remote address");
                let s2 = s.try_clone().expect("Failed to clone the stream");
                return Some((
                    ChannelSender::Remote(Arc::new(Mutex::new(s))),
                    ChannelReceiver::Remote(RefCell::new(s2)),
                    peer_addr,
                ));
            }
        }
    }
}

/// Connect to a remote channel.
pub fn connect_channel<A: ToSocketAddrs, S, R>(
    addr: A,
) -> Result<(ChannelSender<S>, ChannelReceiver<R>), Error> {
    let stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;
    Ok((
        ChannelSender::Remote(Arc::new(Mutex::new(stream))),
        ChannelReceiver::Remote(RefCell::new(stream2)),
    ))
}

#[cfg(test)]
mod tests {
    extern crate pretty_assertions;

    use pretty_assertions::assert_eq;
    use rand::Rng;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        #[derive(Serialize, Deserialize)]
        struct Thing {
            pub x: u32,
            pub y: String,
        }

        let (tx, rx) = new_local_channel();
        tx.send(Thing {
            x: 42,
            y: "foobar".into(),
        })
        .unwrap();
        let thing: Thing = rx.recv().unwrap();
        assert_eq!(thing.x, 42);
        assert_eq!(thing.y, "foobar");
    }

    #[test]
    fn test_remote_channels() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let mut server = ChannelServer::bind(("127.0.0.1", port)).unwrap();
        let client_thread = std::thread::spawn(move || {
            let (sender, receiver) = connect_channel(("127.0.0.1", port)).unwrap();
            sender.send(vec![1, 2, 3, 4]).unwrap();
            let data: Vec<i32> = receiver.recv().unwrap();
            assert_eq!(data, vec![5, 6, 7, 8]);
            sender.send(vec![9, 10, 11, 12]).unwrap();
        });

        let (sender, receiver, _addr) = server.next().unwrap();
        let data: Vec<i32> = receiver.recv().unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
        sender.send(vec![5, 6, 7, 8]).unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![9, 10, 11, 12]);

        client_thread.join().unwrap();
    }
}
