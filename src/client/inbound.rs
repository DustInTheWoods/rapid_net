use super::*;
use crate::ClientEvent;
use rapid_tlv::RapidTlvMessage;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;
use crate::client::error::ClientError;

pub struct InboundClient {
    core: IoCore,
}

impl InboundClient {
    pub fn from_stream(
        stream: TcpStream,
        addr: SocketAddr,
        to_app: Sender<ClientEvent>,
    ) -> Self {
        let core = io_core::IoCore::new(stream, addr, Some(to_app));
        Self { core }
    }

    #[inline] pub fn id(&self) -> Uuid            { self.core.id }
    #[inline] pub fn addr(&self) -> SocketAddr    { self.core.addr }
    #[inline] pub fn is_alive(&self) -> bool      { self.core.is_alive() }

    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        self.core.send(msg).await
    }
}