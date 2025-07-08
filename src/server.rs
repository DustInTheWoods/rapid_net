use std::collections::HashSet;
use std::sync::Arc;
use std::fmt;
use std::error::Error;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;
use dashmap::DashMap;
use rapid_tlv::RapidTlvMessage;
use crate::client::InboundClient;   // 1. neuer Typ
use crate::config::RapidServerConfig;
use crate::message::{ClientEvent, ServerEvent};
use crate::{rapid_debug, rapid_error, rapid_info, rapid_warn};
use futures::future::join_all;   // im Cargo.toml: futures = "0.3"
use log;

/// Error types that can occur during server operations
#[derive(Debug)]
pub enum ServerError {
    /// Indicates that a broadcast operation failed to send a message to all clients
    /// Contains a vector of individual client errors
    BroadcastFailed(Vec<()>),

    /// Indicates that a message could not be encoded before broadcasting
    EncodingFailed,

    /// Indicates that the specified client was not found
    ClientNotFound(Uuid),

    /// Indicates that sending a message to a client failed
    SendFailed(Uuid),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::BroadcastFailed(errors) => {
                write!(f, "Failed to broadcast message to {} clients", errors.len())
            },
            ServerError::EncodingFailed => {
                write!(f, "Failed to encode message for broadcast")
            },
            ServerError::ClientNotFound(client_id) => {
                write!(f, "Client with ID {} not found", client_id)
            },
            ServerError::SendFailed(client_id) => {
                write!(f, "Failed to send message to client {}", client_id)
            }
        }
    }
}

impl Error for ServerError {}

pub struct RapidServer {
    cfg: RapidServerConfig,
    clients: DashMap<Uuid, Arc<InboundClient>>,
}

impl RapidServer {
    pub fn new(cfg: RapidServerConfig) -> Self {
        Self {
            cfg,
            clients: DashMap::new(),
        }
    }

    fn add_client(&self, client: Arc<InboundClient>) {
        self.clients.insert(client.id(), client);
    }

    fn remove_client(&self, client_id: Uuid) {
        rapid_debug!("Removing client {}", client_id);
        
        self.clients.remove(&client_id);
    }

    async fn client_task(self: Arc<Self>, client: Arc<InboundClient>, mut client_rx: Receiver<ClientEvent>, app_tx: Sender<ServerEvent>,) {
        let client_id = client.id();
        rapid_info!("Handling messages for client {}", client_id);

        // Immediately send a Connected event to the application
        // This ensures the server application knows about the client as soon as possible
        let connected_event = ServerEvent::Connected { client_id: client.id() };
        if let Err(e) = app_tx.send(connected_event).await {
            rapid_error!("Failed to send initial connected event for client {}: {:?}", client_id, e);
            return;
        }
        rapid_info!("Sent initial connected event for client {}", client_id);

        while let Some(event) = client_rx.recv().await {
            rapid_info!("Received event from client {}: {:?}", client_id, std::mem::discriminant(&event));

            let server_event = match event {
                ClientEvent::Message { client_id, message } => {
                    rapid_info!("Received message from client {}", client_id);
                    ServerEvent::Message {
                        client_id: client.id(),
                        message,
                    }
                }
                ClientEvent::Connected { client_id } => {
                    rapid_info!("Client {} connected (event from client)", client_id);
                    // We already sent a connected event, so we don't need to send another one, 
                    // But we'll log it for debugging purposes
                    continue;
                }
                ClientEvent::Disconnected { client_id } => {
                    rapid_info!("Client {} disconnected", client_id);
                    ServerEvent::Disconnected { client_id: client.id() }
                }
                ClientEvent::Error { client_id, error } => {
                    rapid_error!("Client {} error: {:?}", client_id, error);
                    ServerEvent::Error { client_id: client.id(), error }
                }
            };

            rapid_info!("Sending event to application for client {}", client_id);
            if let Err(e) = app_tx.send(server_event).await {
                rapid_error!("Failed to forward event from client {}: {:?}", client_id, e);
                break;
            }
            rapid_info!("Sent event to application for client {}", client_id);
        }

        rapid_debug!("Client {} disconnected", client_id);
        app_tx.send(ServerEvent::Disconnected { client_id: client.id() }).await.unwrap();
        self.remove_client(client_id);
    }

    pub async fn run(self: Arc<Self>, tx: Sender<ServerEvent>) {
        let addr = self.cfg.address();
        rapid_info!("Starting server on {}", addr);

        let listener = TcpListener::bind(addr).await.expect("Failed to bind TCP listener");

        // No need for a buffer as we're processing one connection at a time

        loop {
            // Try to accept multiple connections at once if available
            match listener.accept().await {
                Ok((socket, addr)) => {
                    rapid_debug!("Accepted connection from {}", addr);

                    // Set TCP_NODELAY for better performance
                    if let Err(e) = socket.set_nodelay(true) {
                        rapid_warn!("Failed to set TCP_NODELAY: {:?}", e);
                    }

                    // Create a channel with appropriate capacity (100 is usually enough)
                    let (client_tx, client_rx) = tokio::sync::mpsc::channel::<ClientEvent>(100);
                    let client = Arc::new(InboundClient::from_stream(socket, addr, client_tx));

                    self.add_client(Arc::clone(&client));

                    let server = Arc::clone(&self);
                    let app_tx = tx.clone();

                    // Spawn the client task with a name for better debugging
                    tokio::spawn(async move {
                        server.client_task(client, client_rx, app_tx).await;
                    });
                }
                Err(e) => {
                    rapid_error!("Error accepting connection: {:?}", e);
                }
            }
        }
    }

    pub async fn send_to_client(&self, client_id: &Uuid, msg: RapidTlvMessage) -> Result<(), ServerError> {
        rapid_info!("Server sending message to client {}", client_id);
        if let Some(client) = self.clients.get(client_id) {
            rapid_info!("Found client {} in server's client list", client_id);
            match client.send(msg).await {
                Ok(()) => {
                    rapid_info!("Sent message to client {}", client_id);
                    Ok(())
                },
                Err(e) => {
                    rapid_error!("Failed to send message to client {}: {:?}", client_id, e);
                    Err(ServerError::SendFailed(*client_id))
                }
            }
        } else {
            rapid_error!("Failed to find client {} in server's client list", client_id);
            Err(ServerError::ClientNotFound(*client_id))
        }
    }

    pub async fn broadcast(&self, mut msg: RapidTlvMessage, exclude: Option<&HashSet<Uuid>>) -> Result<(), ServerError> {
        let encoded = match msg.encode() {
            Ok(bytes) => bytes.to_vec(),
            Err(_) => return Err(ServerError::EncodingFailed),
        };

        let client_entries: Vec<_> = self
            .clients
            .iter()
            .filter(|entry| exclude.map_or(true, |set| !set.contains(entry.key())))
            .map(|e| (*e.key(), e.value().clone()))
            .collect();

        rapid_debug!(
            "Broadcast start – type 0x{:02X}, size {} B → {} Clients ({} excluded)",
            msg.event_type,
            encoded.len(),
            client_entries.len(),
            exclude.map_or(0, |s| s.len())
        );
        
        let tasks = client_entries.into_iter().map(|(id, client)| {
            let encoded_bytes = encoded.clone();

            async move {
                // Neue Message aus dem gecachten Byte-Slice erzeugen
                let msg = match RapidTlvMessage::parse(bytes::Bytes::from(encoded_bytes)) {
                    Ok(m) => m,
                    Err(_) => {
                        rapid_error!("Broadcast: Re-parse für {id} fehlgeschlagen");
                        return Err((id, ()));
                    }
                };

                match client.send(msg).await {
                    Ok(()) => {
                        rapid_debug!("Broadcast OK → {id}");
                        Ok(())
                    }
                    Err(e) => {
                        rapid_warn!("Broadcast FAIL → {id}: {e:?}");
                        Err((id, ()))
                    }
                }
            }
        });

        let results = join_all(tasks).await;
        
        let mut errors = Vec::new();
        for res in results {
            if let Err((id, e)) = res {
                self.clients.remove(&id);
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            rapid_warn!(
            "Broadcast beendet: {} Fehler / {} verbleibende Clients",
            errors.len(),
            self.clients.len()
        );
            // Nur Fehler melden, wenn *alle* Send-Ops scheiterten
            if errors.len() == self.clients.len() {
                return Err(ServerError::BroadcastFailed(errors));
            }
        } else {
            rapid_info!("Broadcast erfolgreich → {} Clients", self.clients.len());
        }

        Ok(())
    }
}
