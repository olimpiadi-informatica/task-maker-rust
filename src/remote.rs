use anyhow::{bail, Context, Error};
use task_maker_exec::ductile::{
    connect_channel, connect_channel_with_enc, ChannelReceiver, ChannelSender,
};
use url::{ParseError, Url};

use task_maker_exec::derive_key_from_password;

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
    let (server_addrs, password) = match url.scheme() {
        "tcp" => {
            let server_addr = url
                .socket_addrs(|| Some(default_port))
                .context("Cannot resolve server address")?;
            let password = url.password().map(String::from);
            (server_addr, password)
        }
        _ => bail!(
            "Unsupported server address scheme: {}. The supported schemes are: tcp",
            url.scheme()
        ),
    };
    if server_addrs.is_empty() {
        bail!("Cannot resolve server address");
    }
    if !url.path().is_empty() {
        bail!("No path should be provided to the server address");
    }
    let mut err = None;
    for server_addr in server_addrs {
        info!("Connecting to remote server at {}", server_addr);
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
                    debug!("Connection to server failed: {:?}", io_err);
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
    if let Some(err) = err {
        return Err(err.context("Failed to connect to the server"));
    }
    bail!("Unknown error while connecting to the remote server")
}
