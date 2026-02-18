use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Error};
use task_maker_exec::derive_key_from_password;
use task_maker_exec::ductile::{
    connect_channel, connect_channel_with_enc, connect_unix_channel, ChannelReceiver, ChannelSender,
};
use url::{ParseError, Url};

/// Parse the server url address and try to connect to that host.
pub fn connect_to_remote_server<S, R, Str: AsRef<str>>(
    server_url: Str,
    default_port: u16,
) -> Result<(ChannelSender<S>, ChannelReceiver<R>), Error> {
    let url = match Url::parse(server_url.as_ref()) {
        Ok(u) => u,
        Err(ParseError::RelativeUrlWithoutBase) => {
            Url::parse(&format!("tcp://{}", server_url.as_ref())).context("Invalid server url")?
        }
        Err(e) => return Err(e.into()),
    };

    enum Schema {
        Tcp(Vec<SocketAddr>),
        Unix(PathBuf),
    }

    let (schema, password) = match url.scheme() {
        "tcp" => {
            let server_addr = url
                .socket_addrs(|| Some(default_port))
                .context("Cannot resolve server address")?;
            if server_addr.is_empty() {
                bail!("Cannot resolve server address");
            }
            if !url.path().is_empty() {
                bail!("No path should be provided to the server address");
            }
            let password = url.password().map(String::from);
            (Schema::Tcp(server_addr), password)
        }
        "unix" => (Schema::Unix(url.path().into()), None),
        _ => bail!(
            "Unsupported server address scheme: {}. The supported schemes are: tcp, unix",
            url.scheme()
        ),
    };
    let mut err = None;
    match schema {
        Schema::Tcp(server_addrs) => {
            for server_addr in server_addrs {
                info!("Connecting to remote server at {server_addr}");
                let res = match &password {
                    Some(password) => {
                        let key = derive_key_from_password(password);
                        connect_channel_with_enc(server_addr, &key)
                    }
                    None => connect_channel(server_addr),
                };
                match res {
                    Ok(x) => return Ok(x),
                    Err(e) => {
                        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                            debug!("Connection to server failed: {io_err:?}");
                            err = Some(e);
                        } else {
                            err = Some(e);
                            // the error was not due to the network, something went wrong (i.e. wrong
                            // password, wrong version, ...)
                            break;
                        }
                    }
                }
            }
        }
        Schema::Unix(path) => {
            let res = connect_unix_channel(&path).with_context(|| {
                format!("Failed to connect to unix channel at {}", path.display())
            })?;
            return Ok(res);
        }
    }

    if let Some(err) = err {
        return Err(err.context("Failed to connect to the server"));
    }
    bail!("Unknown error while connecting to the remote server")
}
