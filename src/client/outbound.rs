use super::*;
use crate::{rapid_debug, rapid_info, rapid_warn, ClientEvent, RapidClientConfig};
use rapid_tlv::RapidTlvMessage;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;
use uuid::Uuid;
use crate::client::error::ClientError;

pub struct OutboundClient {
    core: io_core::IoCore,
    recv_rx: Receiver<ClientEvent>,
    cfg: RapidClientConfig,
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

        Ok(Self { core, recv_rx: app_rx, cfg: cfg.clone() })
    }

    #[inline] pub fn id(&self) -> Uuid            { self.core.id }
    #[inline] pub fn addr(&self) -> SocketAddr    { self.core.addr }
    #[inline] pub fn is_alive(&self) -> bool      { self.core.is_alive() }

    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        rapid_debug!("Senden: {:?}", msg.event_type);
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

    pub async fn reconnect(&mut self) -> io::Result<()> {
        let mut delay = Duration::from_secs(1);

        loop {
            match Self::connect(&self.cfg).await {
                Ok(fresh) => {
                    rapid_info!("Reconnect zu {} gelungen", fresh.addr());

                    // Felder übernehmen – Drop von self.core schließt alten Socket
                    let OutboundClient { core, recv_rx, cfg } = fresh;
                    self.core = core;
                    self.recv_rx = recv_rx;
                    self.cfg = cfg;
                    return Ok(());
                }
                Err(e) => {
                    rapid_warn!("Reconnect fehlgeschlagen: {e}. Neuer Versuch in {delay:?}");
                    sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                }
            }
        }
    }
}