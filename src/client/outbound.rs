use super::*;
use crate::{rapid_debug, rapid_info, rapid_warn, ClientEvent, RapidClientConfig};
use rapid_tlv::RapidTlvMessage;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::io;
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;
use uuid::Uuid;
use crate::client::error::ClientError;
use crate::config::SocketType;
#[cfg(unix)]
use std::path::Path;

pub struct OutboundClient {
    core: io_core::IoCore,
    recv_rx: Receiver<ClientEvent>,
    cfg: RapidClientConfig,
}

impl OutboundClient {
    pub async fn connect(cfg: &RapidClientConfig) -> io::Result<Self> {
        // Channel for incoming events
        let (app_tx, app_rx) = mpsc::channel::<ClientEvent>(100);

        match cfg.socket_type() {
            SocketType::Tcp => {
                let addr: std::net::SocketAddr = cfg
                    .address()
                    .parse()
                    .map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;

                let stream = TcpStream::connect(addr).await?;
                stream.set_nodelay(cfg.no_delay()).ok();

                // Read loop should run -> Some(app_tx)
                let core = io_core::IoCore::new_tcp(stream, addr, Some(app_tx));

                Ok(Self { core, recv_rx: app_rx, cfg: cfg.clone() })
            },
            SocketType::Unix => {
                #[cfg(unix)]
                {
                    let path = cfg.address();
                    let stream = UnixStream::connect(path).await?;

                    // Read loop should run -> Some(app_tx)
                    let core = io_core::IoCore::new_unix(stream, path.to_string(), Some(app_tx));

                    return Ok(Self { core, recv_rx: app_rx, cfg: cfg.clone() });
                }

                #[cfg(not(unix))]
                {
                    return Err(io::Error::new(
                        ErrorKind::Unsupported,
                        format!("Unix sockets are not supported on this platform: {}", cfg.address())
                    ));
                }
            }
        }
    }

    #[inline] pub fn id(&self) -> Uuid                { self.core.id }
    #[inline] pub fn addr(&self) -> io_core::SocketAddr { self.core.addr.clone() }
    #[inline] pub fn is_alive(&self) -> bool            { self.core.is_alive() }

    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        rapid_debug!("Sending: {:?}", msg.event_type);
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
                    rapid_info!("Reconnect to {} successful", fresh.addr());

                    // Take fields - dropping self.core closes the old socket
                    let OutboundClient { core, recv_rx, cfg } = fresh;
                    self.core = core;
                    self.recv_rx = recv_rx;
                    self.cfg = cfg;
                    return Ok(());
                }
                Err(e) => {
                    rapid_warn!("Reconnect failed: {e}. Retrying in {delay:?}");
                    sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(60));
                }
            }
        }
    }
}
