use std::cell::RefCell;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use chacha20::{ChaCha20, Key, Nonce};
use crossbeam_channel::{unbounded, Receiver, Sender};
use failure::{bail, Error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use chacha20::stream_cipher::NewStreamCipher;
use chacha20::stream_cipher::SyncStreamCipher;

use crate::proto::FileProtocol;
use rand::rngs::OsRng;
use rand::RngCore;
use scrypt::ScryptParams;

const MAGIC: u32 = 0x69696969;

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
    /// The connection is with a remote party, encrypted with ChaCha20.
    RemoteEnc(Arc<Mutex<(TcpStream, ChaCha20)>>),
}

/// The channel part that receives data.
pub enum ChannelReceiver<T> {
    /// The connection is only a local in-memory channel.
    Local(Receiver<ChannelMessage<T>>),
    /// The connection is with a remote party.
    Remote(RefCell<TcpStream>),
    /// The connection is with a remote party and it is encrypted.
    RemoteEnc(RefCell<(TcpStream, ChaCha20)>),
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
            ChannelSender::RemoteEnc(stream) => {
                let mut stream = stream.lock().unwrap();
                let (stream, enc) = stream.deref_mut();
                ChannelSender::<T>::send_remote_raw_enc(stream, enc, ChannelMessage::Message(data))
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
                    Ok(stream.write_all(&data)?)
                }
                FileProtocol::End => {
                    ChannelSender::<T>::send_remote_raw(sender, ChannelMessage::RawFileEnd)
                }
            },
            ChannelSender::RemoteEnc(stream) => {
                let mut stream = stream.lock().unwrap();
                let (stream, enc) = stream.deref_mut();
                match data {
                    FileProtocol::Data(data) => {
                        ChannelSender::<T>::send_remote_raw_enc(
                            stream,
                            enc,
                            ChannelMessage::RawFileData(data.len()),
                        )?;
                        let data = ChannelSender::<T>::encrypt_buffer(data, enc)?;
                        Ok(stream.write_all(&data)?)
                    }
                    FileProtocol::End => Ok(ChannelSender::<T>::send_remote_raw_enc(
                        stream,
                        enc,
                        ChannelMessage::RawFileEnd,
                    )?),
                }
            }
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

    /// Send some unchecked data to the remote channel, encrypting with ChaCha20.
    fn send_remote_raw_enc(
        stream: &mut TcpStream,
        encryptor: &mut ChaCha20,
        data: ChannelMessage<T>,
    ) -> Result<(), Error> {
        let data = bincode::serialize(&data)?;
        let data = ChannelSender::<T>::encrypt_buffer(data, encryptor)?;
        stream.write_all(&data)?;
        Ok(())
    }

    /// Encrypt a buffer, including it's length into a buffer that is a multiple of the encryptor
    /// block length.
    fn encrypt_buffer(mut data: Vec<u8>, encryptor: &mut ChaCha20) -> Result<Vec<u8>, Error> {
        let mut res = Vec::from((data.len() as u32).to_le_bytes());
        res.append(&mut Vec::from(MAGIC.to_le_bytes()));
        res.append(&mut data);
        encryptor.apply_keystream(&mut res);
        Ok(res)
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
            ChannelSender::RemoteEnc(r) => ChannelSender::RemoteEnc(r),
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
            ChannelReceiver::RemoteEnc(receiver) => {
                let mut receiver = receiver.borrow_mut();
                let (receiver, decryptor) = receiver.deref_mut();
                ChannelReceiver::recv_remote_raw_enc(receiver, decryptor)?
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
            ChannelReceiver::RemoteEnc(receiver) => {
                let mut receiver = receiver.borrow_mut();
                let (receiver, decryptor) = receiver.deref_mut();
                match ChannelReceiver::<T>::recv_remote_raw_enc(receiver, decryptor)? {
                    ChannelMessage::RawFileData(_) => {
                        let buf = ChannelReceiver::<T>::decrypt_buffer(receiver, decryptor)?;
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

    /// Receive a message from the encrypted TCP stream of a channel.
    fn recv_remote_raw_enc(
        receiver: &mut TcpStream,
        decryptor: &mut ChaCha20,
    ) -> Result<ChannelMessage<T>, Error> {
        let buf = ChannelReceiver::<T>::decrypt_buffer(receiver, decryptor)?;
        Ok(bincode::deserialize(&buf)?)
    }

    /// Receive and decrypt a frame from the stream.
    fn decrypt_buffer(
        receiver: &mut TcpStream,
        decryptor: &mut ChaCha20,
    ) -> Result<Vec<u8>, Error> {
        let mut len = [0u8; 4];
        receiver.read_exact(&mut len)?;
        decryptor.apply_keystream(&mut len);
        let len = u32::from_le_bytes(len) as usize;

        let mut magic = [0u8; 4];
        receiver.read_exact(&mut magic)?;
        decryptor.apply_keystream(&mut magic);
        let magic = u32::from_le_bytes(magic);
        if magic != MAGIC {
            bail!("Wrong encryption key");
        }

        let mut buf = vec![0u8; len];
        receiver.read_exact(&mut buf)?;
        decryptor.apply_keystream(&mut buf);
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
            ChannelReceiver::RemoteEnc(r) => ChannelReceiver::RemoteEnc(r),
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
    enc_key: Option<[u8; 32]>,
    _sender: PhantomData<*const S>,
    _receiver: PhantomData<*const R>,
}

impl<S, R> ChannelServer<S, R> {
    /// Bind a socket and create a new `ChannelServer`.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer {
            listener: TcpListener::bind(addr)?,
            enc_key: None,
            _sender: Default::default(),
            _receiver: Default::default(),
        })
    }

    /// Bind a socket and create a new `ChannelServer`. All the connection made to this socket must
    /// be encrypted.
    pub fn bind_with_enc<A: ToSocketAddrs>(
        addr: A,
        enc_key: [u8; 32],
    ) -> Result<ChannelServer<S, R>, Error> {
        Ok(ChannelServer {
            listener: TcpListener::bind(addr)?,
            enc_key: Some(enc_key),
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
            if let Ok(mut s) = next {
                let peer_addr = s.peer_addr().expect("Peer has no remote address");
                let s2 = s.try_clone().expect("Failed to clone the stream");
                if let Some(enc_key) = &self.enc_key {
                    let key = Key::from_slice(enc_key);

                    let (enc_nonce, dec_nonce) = match nonce_handshake(&mut s) {
                        Ok(x) => x,
                        Err(e) => {
                            warn!("Nonce handshake failed with {}: {:?}", peer_addr, e);
                            continue;
                        }
                    };
                    let enc_nonce = Nonce::from_slice(&enc_nonce);
                    let mut enc = ChaCha20::new(&key, &enc_nonce);

                    let dec_nonce = Nonce::from_slice(&dec_nonce);
                    let mut dec = ChaCha20::new(&key, &dec_nonce);

                    if let Err(e) = check_encryption_key(&mut s, &mut enc, &mut dec) {
                        warn!("Magic handshake failed with {}: {:?}", peer_addr, e);
                        continue;
                    }

                    return Some((
                        ChannelSender::RemoteEnc(Arc::new(Mutex::new((s, enc)))),
                        ChannelReceiver::RemoteEnc(RefCell::new((s2, dec))),
                        peer_addr,
                    ));
                } else {
                    if let Err(e) = check_no_encryption(&mut s) {
                        warn!("Magic handshake failed with {}: {:?}", peer_addr, e);
                        continue;
                    }
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
    let mut stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;
    check_no_encryption(&mut stream)?;
    Ok((
        ChannelSender::Remote(Arc::new(Mutex::new(stream))),
        ChannelReceiver::Remote(RefCell::new(stream2)),
    ))
}

/// Connect to a remote channel encrypting the connection.
pub fn connect_channel_with_enc<A: ToSocketAddrs, S, R>(
    addr: A,
    enc_key: &[u8; 32],
) -> Result<(ChannelSender<S>, ChannelReceiver<R>), Error> {
    let mut stream = TcpStream::connect(addr)?;
    let stream2 = stream.try_clone()?;

    let (enc_nonce, dec_nonce) = nonce_handshake(&mut stream)?;
    let key = Key::from_slice(enc_key);
    let mut enc = ChaCha20::new(&key, &Nonce::from_slice(&enc_nonce));
    let mut dec = ChaCha20::new(&key, &Nonce::from_slice(&dec_nonce));

    check_encryption_key(&mut stream, &mut enc, &mut dec)?;

    Ok((
        ChannelSender::RemoteEnc(Arc::new(Mutex::new((stream, enc)))),
        ChannelReceiver::RemoteEnc(RefCell::new((stream2, dec))),
    ))
}

/// Derive the encryption key from a password string.
pub fn derive_key_from_password<S: AsRef<str>>(password: S) -> [u8; 32] {
    let mut key = [0u8; 32];
    scrypt::scrypt(
        password.as_ref().as_bytes(),
        b"task-maker",
        &ScryptParams::new(8, 8, 1).unwrap(),
        &mut key,
    )
    .expect("Failed to derive key from password");
    key
}

/// Send the encryption nonce and receive the decryption nonce using the provided socket.
fn nonce_handshake(s: &mut TcpStream) -> Result<([u8; 12], [u8; 12]), Error> {
    let mut enc_nonce = [0u8; 12];
    OsRng.fill_bytes(&mut enc_nonce);
    s.write_all(&enc_nonce)?;
    s.flush()?;

    let mut dec_nonce = [0u8; 12];
    s.read_exact(&mut dec_nonce)?;

    Ok((enc_nonce, dec_nonce))
}

/// Check that the encryption key is the same both ends.
fn check_encryption_key(
    stream: &mut TcpStream,
    enc: &mut ChaCha20,
    dec: &mut ChaCha20,
) -> Result<(), Error> {
    let mut magic = MAGIC.to_le_bytes();
    enc.apply_keystream(&mut magic);
    stream.write_all(&magic)?;
    stream.flush()?;

    stream.read_exact(&mut magic)?;
    dec.apply_keystream(&mut magic);
    let magic = u32::from_le_bytes(magic);
    if magic != MAGIC {
        bail!("Wrong encryption key");
    }
    Ok(())
}

/// Check that no encryption is used by the other end.
fn check_no_encryption(stream: &mut TcpStream) -> Result<(), Error> {
    let key = b"task-maker's the best thing ever";
    let nonce = b"task-maker!!";
    let mut enc = ChaCha20::new(Key::from_slice(key), Nonce::from_slice(nonce));
    let mut dec = ChaCha20::new(Key::from_slice(key), Nonce::from_slice(nonce));
    check_encryption_key(stream, &mut enc, &mut dec)
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
    fn test_remote_channels_emc() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let enc_key = [69u8; 32];
        let mut server = ChannelServer::bind_with_enc(("127.0.0.1", port), enc_key).unwrap();
        let client_thread = std::thread::spawn(move || {
            let (sender, receiver) =
                connect_channel_with_enc(("127.0.0.1", port), &enc_key).unwrap();
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
    fn test_remote_channels_enc_wrong_key() {
        let port = rand::thread_rng().gen_range(10000u16, 20000u16);
        let enc_key = [42u8; 32];
        let mut server: ChannelServer<Vec<u8>, Vec<u8>> =
            ChannelServer::bind_with_enc(("127.0.0.1", port), enc_key).unwrap();
        let client_thread = std::thread::spawn(move || {
            let wrong_enc_key = [69u8; 32];
            assert!(
                connect_channel_with_enc::<_, (), ()>(("127.0.0.1", port), &wrong_enc_key).is_err()
            );
            // the call to .next() below blocks until a client connects successfully
            connect_channel_with_enc::<_, (), ()>(("127.0.0.1", port), &enc_key).unwrap();
        });

        server.next().unwrap();
        client_thread.join().unwrap();
    }
}
