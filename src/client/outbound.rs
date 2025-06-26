use super::*;
use crate::{ClientEvent, RapidClientConfig};
use rapid_tlv::RapidTlvMessage;
use std::io::ErrorKind;
use std::net::SocketAddr;
use tokio::io;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;
use crate::client::error::ClientError;

pub struct OutboundClient {
    core: io_core::IoCore,
    recv_rx: Receiver<ClientEvent>,
}

impl OutboundClient {
    pub async fn connect(cfg: &RapidClientConfig) -> io::Result<Self> {
        let addr: SocketAddr = cfg
            .address()
            .parse()
            .map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;   // ← hier

        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(cfg.no_delay()).ok();

        // Channel für eingehende Events
        let (app_tx, app_rx) = mpsc::channel::<ClientEvent>(100);

        // Read-Loop soll laufen → Some(app_tx)
        let core = io_core::IoCore::new(stream, addr, Some(app_tx));

        Ok(Self { core, recv_rx: app_rx })
    }

    #[inline] pub fn id(&self) -> Uuid            { self.core.id }
    #[inline] pub fn addr(&self) -> SocketAddr    { self.core.addr }
    #[inline] pub fn is_alive(&self) -> bool      { self.core.is_alive() }

    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        self.core.send(msg).await
    }

    pub async fn recv(&mut self) -> Option<RapidTlvMessage> {
        while let Some(ev) = self.recv_rx.recv().await {
            if let ClientEvent::Message { message, .. } = ev {
                return Some(message);
            }
        }
        None
    }
}