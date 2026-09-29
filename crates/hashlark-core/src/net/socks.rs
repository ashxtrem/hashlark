// SPDX-License-Identifier: GPL-3.0-or-later

//! A minimal local SOCKS5 server (RFC 1928: no authentication, CONNECT
//! only) that hands each connection to a [`Connector`]. Embedded Tor uses it
//! so that every HTTP client can reach Tor through an ordinary
//! `socks5h://127.0.0.1:<port>` proxy.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A bidirectional byte stream.
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

pub type ConnectFuture = Pin<Box<dyn Future<Output = io::Result<Box<dyn AsyncStream>>> + Send>>;

/// Opens the outgoing connection for a SOCKS request.
pub trait Connector: Send + Sync + 'static {
    fn connect(&self, host: String, port: u16) -> ConnectFuture;
}

const VERSION: u8 = 5;
const NO_AUTH: u8 = 0;
const NO_ACCEPTABLE_METHOD: u8 = 0xFF;
const CMD_CONNECT: u8 = 1;
const REPLY_OK: u8 = 0;
const REPLY_FAILURE: u8 = 1;
const REPLY_HOST_UNREACHABLE: u8 = 4;
const REPLY_COMMAND_NOT_SUPPORTED: u8 = 7;
const REPLY_ADDRESS_NOT_SUPPORTED: u8 = 8;

/// Binds `127.0.0.1:0` and serves SOCKS5 until the task is aborted.
/// Returns the bound address and the server task.
pub async fn start(
    connector: Arc<dyn Connector>,
) -> io::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let connector = Arc::clone(&connector);
                    tokio::spawn(async move {
                        if let Err(e) = handle(stream, connector).await {
                            tracing::debug!(error = %e, "SOCKS connection ended with an error");
                        }
                    });
                }
                Err(e) => tracing::warn!(error = %e, "SOCKS accept failed"),
            }
        }
    });
    Ok((addr, task))
}

async fn reply(stream: &mut TcpStream, code: u8) -> io::Result<()> {
    // Bound address: 0.0.0.0:0 (clients ignore it for CONNECT).
    stream
        .write_all(&[VERSION, code, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
}

async fn handle(mut client: TcpStream, connector: Arc<dyn Connector>) -> io::Result<()> {
    // Greeting: VER NMETHODS METHODS...
    let mut head = [0u8; 2];
    client.read_exact(&mut head).await?;
    if head[0] != VERSION {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not SOCKS5"));
    }
    let mut methods = vec![0u8; usize::from(head[1])];
    client.read_exact(&mut methods).await?;
    if !methods.contains(&NO_AUTH) {
        client.write_all(&[VERSION, NO_ACCEPTABLE_METHOD]).await?;
        return Ok(());
    }
    client.write_all(&[VERSION, NO_AUTH]).await?;

    // Request: VER CMD RSV ATYP DST.ADDR DST.PORT. Read all of it before
    // answering: closing with unread data resets the connection.
    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    let host = match request[3] {
        1 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        3 => {
            let len = client.read_u8().await?;
            let mut name = vec![0u8; usize::from(len)];
            client.read_exact(&mut name).await?;
            String::from_utf8(name)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad host name"))?
        }
        4 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            Ipv6Addr::from(ip).to_string()
        }
        _ => {
            reply(&mut client, REPLY_ADDRESS_NOT_SUPPORTED).await?;
            return Ok(());
        }
    };
    let port = client.read_u16().await?;
    if request[1] != CMD_CONNECT {
        reply(&mut client, REPLY_COMMAND_NOT_SUPPORTED).await?;
        return Ok(());
    }

    let mut remote = match connector.connect(host, port).await {
        Ok(remote) => remote,
        Err(e) => {
            let code = if e.kind() == io::ErrorKind::NotFound {
                REPLY_HOST_UNREACHABLE
            } else {
                REPLY_FAILURE
            };
            reply(&mut client, code).await?;
            return Ok(());
        }
    };
    reply(&mut client, REPLY_OK).await?;
    tokio::io::copy_bidirectional(&mut client, &mut remote).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Connects directly, recording the requested host.
    struct Direct(std::sync::Mutex<Vec<String>>);

    impl Connector for Direct {
        fn connect(&self, host: String, port: u16) -> ConnectFuture {
            self.0.lock().unwrap().push(host.clone());
            Box::pin(async move {
                let stream = TcpStream::connect((host.as_str(), port)).await?;
                Ok(Box::new(stream) as Box<dyn AsyncStream>)
            })
        }
    }

    #[tokio::test]
    async fn reqwest_works_through_the_bridge() {
        let server = MockServer::start().await;
        Mock::given(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("through socks"))
            .mount(&server)
            .await;
        let connector = Arc::new(Direct(Default::default()));
        let (addr, task) = start(connector.clone()).await.unwrap();

        let client = crate::net::build_client(&crate::net::ClientOptions {
            proxy: Some(format!("socks5h://{addr}").parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
        // socks5h: the name is resolved on the far side of the proxy.
        let url = server.uri().replace("127.0.0.1", "localhost");
        let body = client
            .get(format!("{url}/hello"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "through socks");
        assert_eq!(connector.0.lock().unwrap().as_slice(), ["localhost"]);
        task.abort();
    }

    #[tokio::test]
    async fn rejects_unsupported_commands() {
        let (addr, task) = start(Arc::new(Direct(Default::default()))).await.unwrap();
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(&[5, 1, 0]).await.unwrap();
        let mut hello = [0u8; 2];
        s.read_exact(&mut hello).await.unwrap();
        assert_eq!(hello, [5, 0]);
        // BIND (2) isn't supported.
        s.write_all(&[5, 2, 0, 1, 127, 0, 0, 1, 0, 80])
            .await
            .unwrap();
        let mut answer = [0u8; 10];
        s.read_exact(&mut answer).await.unwrap();
        assert_eq!(answer[1], REPLY_COMMAND_NOT_SUPPORTED);
        task.abort();
    }
}
