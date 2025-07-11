use std::collections::HashSet;
use std::sync::Arc;
use std::fmt;
use std::error::Error;
#[cfg(unix)]
use std::path::Path;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::unix::SocketAddr as UnixSocketAddr;
use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;
use dashmap::DashMap;
use rapid_tlv::RapidTlvMessage;
use crate::client::InboundClient;
use crate::config::{RapidServerConfig, SocketType};
use crate::message::{ClientEvent, ServerEvent};
use crate::{rapid_debug, rapid_error, rapid_info, rapid_warn};
use futures::future::join_all;

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
        rapid_debug!("Handling messages for client {}", client_id);

        // Immediately send a Connected event to the application
        // This ensures the server application knows about the client as soon as possible
        let connected_event = ServerEvent::Connected { client_id };
        if let Err(e) = app_tx.send(connected_event).await {
            rapid_error!("Failed to send initial connected event for client {}: {:?}", client_id, e);
            return;
        }
        rapid_debug!("Sent initial connected event for client {}", client_id);

        while let Some(event) = client_rx.recv().await {
            rapid_debug!("Received event from client {}: {:?}", client_id, std::mem::discriminant(&event));

            let server_event = match event {
                ClientEvent::Message { client_id: event_client_id, message } => {
                    rapid_debug!("Received message from client {}", event_client_id);
                    ServerEvent::Message {
                        client_id,
                        message,
                    }
                }
                ClientEvent::Connected { client_id: event_client_id } => {
                    rapid_debug!("Client {} connected (event from client)", event_client_id);
                    // We already sent a connected event, so we don't need to send another one, 
                    // But we'll log it for debugging purposes
                    continue;
                }
                ClientEvent::Disconnected { client_id: event_client_id } => {
                    rapid_info!("Client {} disconnected", event_client_id);
                    ServerEvent::Disconnected { client_id }
                }
                ClientEvent::Error { client_id: event_client_id, error } => {
                    rapid_error!("Client {} error: {:?}", event_client_id, error);
                    ServerEvent::Error { client_id, error }
                }
            };

            rapid_debug!("Forwarding event to application for client {}", client_id);
            if let Err(e) = app_tx.send(server_event).await {
                rapid_error!("Failed to forward event from client {}: {:?}", client_id, e);
                break;
            }
        }

        rapid_info!("Client {} disconnected, removing from server", client_id);
        app_tx.send(ServerEvent::Disconnected { client_id }).await.unwrap();
        self.remove_client(client_id);
    }

    pub async fn run(self: Arc<Self>, tx: Sender<ServerEvent>) {
        let addr = self.cfg.address();
        let socket_type = self.cfg.socket_type();
        rapid_info!("Starting server on {} using {:?}", addr, socket_type);

        match socket_type {
            SocketType::Tcp => {
                let self_clone = Arc::clone(&self);
                self_clone.run_tcp(addr, tx).await;
            },
            SocketType::Unix => {
                #[cfg(unix)]
                {
                    let self_clone = Arc::clone(&self);
                    self_clone.run_unix(addr, tx).await;
                }
                #[cfg(not(unix))]
                {
                    rapid_error!("Unix sockets are not supported on this platform");
                    panic!("Unix sockets are not supported on this platform");
                }
            }
        }
    }

    async fn run_tcp(self: Arc<Self>, addr: &str, tx: Sender<ServerEvent>) {
        let listener = TcpListener::bind(addr).await.expect("Failed to bind TCP listener");

        // No need for a buffer as we're processing one connection at a time

        loop {
            // Accept new connections
            match listener.accept().await {
                Ok((socket, addr)) => {
                    rapid_info!("Accepted TCP connection from {}", addr);

                    // Set TCP_NODELAY for better performance
                    if let Err(e) = socket.set_nodelay(self.cfg.no_delay()) {
                        rapid_warn!("Failed to set TCP_NODELAY for {}: {:?}", addr, e);
                    }

                    // Create a channel with appropriate capacity (100 is usually enough)
                    let (client_tx, client_rx) = tokio::sync::mpsc::channel::<ClientEvent>(100);
                    let client = Arc::new(InboundClient::from_tcp_stream(socket, addr, client_tx));

                    self.add_client(Arc::clone(&client));
                    rapid_debug!("Added client {} to server", client.id());

                    let server = Arc::clone(&self);
                    let app_tx = tx.clone();

                    // Spawn the client task directly
                    tokio::spawn(async move {
                        server.client_task(client, client_rx, app_tx).await;
                    });
                }
                Err(e) => {
                    rapid_error!("Error accepting TCP connection: {:?}", e);
                }
            }
        }
    }

    #[cfg(unix)]
    #[allow(dead_code)]
    async fn run_unix(self: Arc<Self>, addr: &str, tx: Sender<ServerEvent>) {
        // Make sure the path exists and is accessible
        if let Some(parent) = Path::new(addr).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).expect("Failed to create directory for Unix socket");
            }
        }

        // Remove the socket file if it already exists
        if Path::new(addr).exists() {
            std::fs::remove_file(addr).expect("Failed to remove existing Unix socket");
        }

        let listener = UnixListener::bind(addr).expect("Failed to bind Unix listener");

        loop {
            // Accept new connections
            match listener.accept().await {
                Ok((socket, _)) => {
                    rapid_info!("Accepted Unix socket connection");

                    // Create a channel with appropriate capacity (100 is usually enough)
                    let (client_tx, client_rx) = tokio::sync::mpsc::channel::<ClientEvent>(100);
                    let client = Arc::new(InboundClient::from_unix_stream(socket, addr.to_string(), client_tx));

                    self.add_client(Arc::clone(&client));
                    rapid_debug!("Added client {} to server", client.id());

                    let server = Arc::clone(&self);
                    let app_tx = tx.clone();

                    // Spawn the client task directly
                    tokio::spawn(async move {
                        server.client_task(client, client_rx, app_tx).await;
                    });
                }
                Err(e) => {
                    rapid_error!("Error accepting Unix socket connection: {:?}", e);
                }
            }
        }
    }

    #[cfg(not(unix))]
    async fn run_unix(self: Arc<Self>, _addr: &str, _tx: Sender<ServerEvent>) {
        // This method is only a stub for non-Unix platforms
        // The actual implementation is conditionally compiled for Unix platforms only
        unreachable!("Unix sockets are not supported on this platform");
    }

    pub async fn send_to_client(&self, client_id: &Uuid, msg: RapidTlvMessage) -> Result<(), ServerError> {
        rapid_debug!("Sending message to client {}", client_id);
        if let Some(client) = self.clients.get(client_id) {
            match client.send(msg).await {
                Ok(()) => {
                    rapid_debug!("Successfully sent message to client {}", client_id);
                    Ok(())
                },
                Err(e) => {
                    rapid_error!("Failed to send message to client {}: {:?}", client_id, e);
                    Err(ServerError::SendFailed(*client_id))
                }
            }
        } else {
            rapid_warn!("Client {} not found in server's client list", client_id);
            Err(ServerError::ClientNotFound(*client_id))
        }
    }

    pub async fn broadcast(&self, mut msg: RapidTlvMessage, exclude: Option<&HashSet<Uuid>>) -> Result<(), ServerError> {
        // Encode the message once
        let encoded = match msg.encode() {
            Ok(bytes) => bytes.to_vec(),
            Err(_) => return Err(ServerError::EncodingFailed),
        };

        // Get all clients that should receive the broadcast
        let client_entries: Vec<_> = self
            .clients
            .iter()
            .filter(|entry| exclude.map_or(true, |set| !set.contains(entry.key())))
            .map(|e| (*e.key(), e.value().clone()))
            .collect();

        rapid_info!(
            "Broadcasting message type 0x{:02X}, size {} bytes to {} clients ({} excluded)",
            msg.event_type,
            encoded.len(),
            client_entries.len(),
            exclude.map_or(0, |s| s.len())
        );

        // Process clients in batches to avoid stack overflow
        const BATCH_SIZE: usize = 100;
        let mut errors = Vec::new();

        // Process clients in batches
        for chunk in client_entries.chunks(BATCH_SIZE) {
            let tasks = chunk.iter().map(|(id, client)| {
                let encoded_bytes = encoded.clone();
                let id = *id;
                let client = client.clone();

                async move {
                    // Create a new message from the cached byte slice
                    let msg = match RapidTlvMessage::parse(bytes::Bytes::from(encoded_bytes)) {
                        Ok(m) => m,
                        Err(_) => {
                            rapid_error!("Broadcast: Re-parse failed for client {id}");
                            return Err((id, ()));
                        }
                    };

                    match client.send(msg).await {
                        Ok(()) => {
                            rapid_debug!("Broadcast successful to client {id}");
                            Ok(())
                        }
                        Err(e) => {
                            rapid_warn!("Broadcast failed to client {id}: {e:?}");
                            Err((id, ()))
                        }
                    }
                }
            });

            // Process this batch
            let batch_results = join_all(tasks).await;

            // Collect errors from this batch
            for res in batch_results {
                if let Err((id, e)) = res {
                    self.clients.remove(&id);
                    errors.push(e);
                }
            }
        }

        if !errors.is_empty() {
            rapid_warn!(
                "Broadcast completed with {} errors, {} remaining clients",
                errors.len(),
                self.clients.len()
            );
            // Only report an error if all send operations failed
            if errors.len() == client_entries.len() {
                return Err(ServerError::BroadcastFailed(errors));
            }
        } else {
            rapid_info!("Broadcast successfully completed to {} clients", self.clients.len());
        }

        Ok(())
    }
}
