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
use crypto::aessafe::{AesSafe128Decryptor, AesSafe128Encryptor};
use crypto::symmetriccipher::{BlockDecryptor, BlockEncryptor};

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
#[derive(Clone)]
pub enum ChannelSender<T> {
    /// The connection is only a local in-memory channel.
    Local(Sender<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(Arc<Mutex<TcpStream>>),
    /// The connection is with a remote party, encrypted with AES.
    RemoteAes(Arc<Mutex<TcpStream>>, AesSafe128Encryptor),
}

/// The channel part that receives data.
pub enum ChannelReceiver<T> {
    /// The connection is only a local in-memory channel.
    Local(Receiver<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(RefCell<TcpStream>),
    /// The connection is with a remote party, encrypted with AES.
    RemoteAes(RefCell<TcpStream>, AesSafe128Decryptor),
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
            ChannelSender::RemoteAes(sender, encryptor) => ChannelSender::<T>::send_remote_raw_aes(
                sender,
                encryptor,
                ChannelMessage::Message(data),
            ),
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
                    Ok(stream.write_all(&data)?)
                }
                FileProtocol::End => {
                    ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::RawFileEnd)
                }
            },
            ChannelSender::RemoteAes(sender, encryptor) => match data {
                FileProtocol::Data(data) => {
                    ChannelSender::<T>::send_remote_raw_aes(
                        sender,
                        encryptor,
                        ChannelMessage::RawFileData(data.len()),
                    )?;
                    let enc = ChannelSender::<T>::encrypt_buffer(data, encryptor);
                    let mut sender = sender.lock().expect("Cannot lock ChannelSender");
                    let stream = sender.deref_mut();
                    Ok(stream.write_all(&enc)?)
                }
                FileProtocol::End => Ok(ChannelSender::<T>::send_remote_raw_aes(
                    sender,
                    encryptor,
                    ChannelMessage::RawFileEnd,
                )?),
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

    /// Send some unchecked data to the remote channel, encrypting with aes.
    fn send_remote_raw_aes<E: BlockEncryptor>(
        sender: &Arc<Mutex<TcpStream>>,
        encryptor: &E,
        data: ChannelMessage<T>,
    ) -> Result<(), Error> {
        let data = ChannelSender::<T>::encrypt_buffer(bincode::serialize(&data)?, encryptor);

        let mut sender = sender.lock().expect("Cannot lock ChannelSender");
        let stream = sender.deref_mut();

        stream.write_all(&data)?;
        Ok(())
    }

    /// Encrypt a buffer, including it's length into a buffer that is a multiple of the encryptor
    /// block length.
    fn encrypt_buffer<E: BlockEncryptor>(mut data: Vec<u8>, encryptor: &E) -> Vec<u8> {
        let block_size = encryptor.block_size();

        let mut buf = Vec::from((data.len() as u32).to_le_bytes());
        // magic string
        buf.push(69);
        buf.push(69);
        buf.push(69);
        buf.push(69);
        buf.append(&mut data);
        let pad_len = (block_size - buf.len() % block_size) % block_size;
        buf.resize(buf.len() + pad_len, 0);
        debug_assert!(buf.len() % block_size == 0);

        let mut tmp = vec![0u8; block_size];
        for i in 0..buf.len() / block_size {
            encryptor.encrypt_block(&buf[block_size * i..block_size * (i + 1)], &mut tmp);
            buf[block_size * i..block_size * (i + 1)].clone_from_slice(&tmp);
        }
        buf
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
            ChannelSender::RemoteAes(r, e) => ChannelSender::RemoteAes(r, e),
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
            ChannelReceiver::RemoteAes(receiver, decryptor) => {
                ChannelReceiver::recv_remote_raw_aes(receiver, decryptor)?
            }
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
            ChannelReceiver::RemoteAes(receiver, decryptor) => {
                match ChannelReceiver::<T>::recv_remote_raw_aes(receiver, decryptor)? {
                    ChannelMessage::RawFileData(_) => {
                        let mut receiver = receiver.borrow_mut();
                        let buf =
                            ChannelReceiver::<T>::decrypt_buffer(receiver.deref_mut(), decryptor)?;
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

    fn recv_remote_raw_aes<D: BlockDecryptor>(
        receiver: &RefCell<TcpStream>,
        decryptor: &D,
    ) -> Result<ChannelMessage<T>, Error> {
        let mut receiver = receiver.borrow_mut();
        let receiver = receiver.deref_mut();
        let buf = ChannelReceiver::<T>::decrypt_buffer(receiver, decryptor)?;
        Ok(bincode::deserialize(&buf)?)
    }

    fn decrypt_buffer<D: BlockDecryptor>(
        receiver: &mut TcpStream,
        decryptor: &D,
    ) -> Result<Vec<u8>, Error> {
        let block_size = decryptor.block_size();

        let mut block = vec![0u8; block_size];
        let mut encrypted_block = vec![0u8; block_size];

        // the first block contains the buffer size
        receiver.read_exact(&mut encrypted_block)?;
        decryptor.decrypt_block(&encrypted_block, &mut block);

        // extract the buffer size
        let len = [block[0], block[1], block[2], block[3]];
        let len = (u32::from_le_bytes(len)) as usize;
        for i in 4..8 {
            if block[i] != 69 {
                bail!("Wrong encryption key");
            }
        }

        // read and decrypt all the blocks
        let mut buf = vec![];
        buf.append(&mut Vec::from(&block[8..]));
        while buf.len() < len {
            receiver.read_exact(&mut encrypted_block)?;
            decryptor.decrypt_block(&encrypted_block, &mut block);
            buf.append(&mut block.clone());
        }
        // remove the padding
        buf.resize(len, 0);
        Ok(buf)
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
            ChannelReceiver::RemoteAes(r, d) => ChannelReceiver::RemoteAes(r, d),
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
pub struct ChannelServer<S, R> {
    listener: TcpListener,
    aes_key: Option<Vec<u8>>,
    _sender: PhantomData<*const S>,
    _receiver: PhantomData<*const R>,
}

impl<S, R> ChannelServer<S, R> {
    /// Bind a socket and create a new `ChannelServer`.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer {
            listener: TcpListener::bind(addr)?,
            aes_key: None,
            _sender: Default::default(),
            _receiver: Default::default(),
        })
    }

    /// Bind a socket and create a new `ChannelServer`. All the connection made to this socket must
    /// be encrypted using AES.
    pub fn bind_with_aes<A: ToSocketAddrs>(
        addr: A,
        aes_key: Vec<u8>,
    ) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer {
            listener: TcpListener::bind(addr)?,
            aes_key: Some(aes_key),
            _sender: Default::default(),
            _receiver: Default::default(),
        })
    }
}

impl<S, R> Deref for ChannelServer<S, R> {
    type Target = TcpListener;

    fn deref(&self) -> &Self::Target {
        &self.listener
    }
}

impl<S, R> Iterator for ChannelServer<S, R> {
    type Item = (ChannelSender<S>, ChannelReceiver<R>, SocketAddr);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self
                .listener
                .incoming()
                .next()
                .expect("TcpListener::incoming returned None");
            if let Ok(s) = next {
                let peer_addr = s.peer_addr().expect("Peer has no remote address");
                let s2 = s.try_clone().expect("Failed to clone the stream");
                if let Some(aes_key) = &self.aes_key {
                    let encryptor = AesSafe128Encryptor::new(aes_key);
                    let decryptor = AesSafe128Decryptor::new(aes_key);

                    return Some((
                        ChannelSender::RemoteAes(Arc::new(Mutex::new(s)), encryptor),
                        ChannelReceiver::RemoteAes(RefCell::new(s2), decryptor),
                        peer_addr,
                    ));
                } else {
                    return Some((
                        ChannelSender::Remote(Arc::new(Mutex::new(s))),
                        ChannelReceiver::Remote(RefCell::new(s2)),
                        peer_addr,
                    ));
                }
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

/// Connect to a remote channel encrypting with AES.
pub fn connect_channel_with_aes<A: ToSocketAddrs, S, R>(
    addr: A,
    aes_key: &[u8],
) -> Result<(ChannelSender<S>, ChannelReceiver<R>), Error> {
    let stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;

    let encryptor = AesSafe128Encryptor::new(aes_key);
    let decryptor = AesSafe128Decryptor::new(aes_key);

    Ok((
        ChannelSender::RemoteAes(Arc::new(Mutex::new(stream)), encryptor),
        ChannelReceiver::RemoteAes(RefCell::new(stream2), decryptor),
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
            sender
                .send_file(FileProtocol::Data(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]))
                .unwrap();
            sender.send_file(FileProtocol::End).unwrap();
        });

        let (sender, receiver, _addr) = server.next().unwrap();
        let data: Vec<i32> = receiver.recv().unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
        sender.send(vec![5, 6, 7, 8]).unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![9, 10, 11, 12]);
        let file = receiver.recv_file().unwrap();
        assert_eq!(file, FileProtocol::Data(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]));
        let file = receiver.recv_file().unwrap();
        assert_eq!(file, FileProtocol::End);

        client_thread.join().unwrap();
    }

    #[test]
    fn test_remote_channels_aes() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let aes_key = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let mut server =
            ChannelServer::bind_with_aes(("127.0.0.1", port), aes_key.clone()).unwrap();
        let client_thread = std::thread::spawn(move || {
            let aes_key = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
            let (sender, receiver) =
                connect_channel_with_aes(("127.0.0.1", port), &aes_key).unwrap();
            sender.send(vec![1u8, 2, 3, 4]).unwrap();
            let data: Vec<u8> = receiver.recv().unwrap();
            assert_eq!(data, vec![5u8, 6, 7, 8]);
            sender.send(vec![69u8; 12345]).unwrap();
            sender
                .send_file(FileProtocol::Data(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]))
                .unwrap();
            sender.send_file(FileProtocol::End).unwrap();
        });

        let (sender, receiver, _addr) = server.next().unwrap();
        let data: Vec<u8> = receiver.recv().unwrap();
        assert_eq!(data, vec![1u8, 2, 3, 4]);
        sender.send(vec![5u8, 6, 7, 8]).unwrap();
        let data = receiver.recv().unwrap();
        assert_eq!(data, vec![69u8; 12345]);
        let file = receiver.recv_file().unwrap();
        assert_eq!(file, FileProtocol::Data(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]));
        let file = receiver.recv_file().unwrap();
        assert_eq!(file, FileProtocol::End);

        client_thread.join().unwrap();
    }

    #[test]
    fn test_remote_channels_aes_wrong_key() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let aes_key = vec![42u8; 16];
        let mut server: ChannelServer<Vec<u8>, Vec<u8>> =
            ChannelServer::bind_with_aes(("127.0.0.1", port), aes_key.clone()).unwrap();
        let client_thread = std::thread::spawn(move || {
            let aes_key = vec![69u8; 16];
            let (sender, receiver): (_, ChannelReceiver<Vec<u8>>) =
                connect_channel_with_aes(("127.0.0.1", port), &aes_key).unwrap();
            sender.send(vec![69u8; 12345]).unwrap();
            assert!(receiver.recv().is_err());
        });

        let (sender, receiver, _addr) = server.next().unwrap();
        sender.send(vec![69u8; 12345]).unwrap();
        assert!(receiver.recv().is_err());

        client_thread.join().unwrap();
    }
}
