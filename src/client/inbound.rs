use super::*;
use crate::ClientEvent;
use rapid_tlv::RapidTlvMessage;
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;
use crate::client::error::ClientError;
#[cfg(unix)]
use std::path::Path;

pub struct InboundClient {
    core: IoCore,
}

impl InboundClient {
    pub fn from_tcp_stream(
        stream: TcpStream,
        addr: std::net::SocketAddr,
        to_app: Sender<ClientEvent>,
    ) -> Self {
        let core = IoCore::new_tcp(stream, addr, Some(to_app));
        Self { core }
    }

    #[cfg(unix)]
    pub fn from_unix_stream(
        stream: UnixStream,
        path: String,
        to_app: Sender<ClientEvent>,
    ) -> Self {
        let core = IoCore::new_unix(stream, path, Some(to_app));
        Self { core }
    }

    #[cfg(not(unix))]
    pub fn from_unix_stream(
        _stream: std::io::Error, // We use Error as a placeholder since UnixStream doesn't exist on non-Unix
        path: String,
        _to_app: Sender<ClientEvent>,
    ) -> Self {
        panic!("Unix sockets are not supported on this platform: {}", path);
    }

    pub fn from_stream(
        stream: TcpStream,
        addr: std::net::SocketAddr,
        to_app: Sender<ClientEvent>,
    ) -> Self {
        // For backward compatibility
        Self::from_tcp_stream(stream, addr, to_app)
    }

    #[inline] pub fn id(&self) -> Uuid                { self.core.id }
    #[inline] pub fn addr(&self) -> io_core::SocketAddr { self.core.addr.clone() }
    #[inline] pub fn is_alive(&self) -> bool            { self.core.is_alive() }

    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        self.core.send(msg).await
    }
}
