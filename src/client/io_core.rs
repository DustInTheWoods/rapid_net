use crate::{rapid_debug, rapid_error, rapid_info, rapid_warn, ClientEvent};
use bytes::BytesMut;
use rapid_tlv::RapidTlvMessage;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::mpsc::{Receiver, Sender},
};
use uuid::Uuid;
use crate::client::error::ClientError;

pub(crate) struct IoCore {
    pub id:   Uuid,
    pub addr: SocketAddr,
    write_tx: Sender<RapidTlvMessage>,
    read_jh:  tokio::task::JoinHandle<()>,
    write_jh: tokio::task::JoinHandle<()>,
}

impl IoCore {
    pub fn new(
        stream: TcpStream,
        addr: SocketAddr,
        to_app: Option<Sender<ClientEvent>>, // nur Inbound braucht das
    ) -> Self {
        let (reader, writer) = stream.into_split();
        let (write_tx, write_rx) = mpsc::channel::<RapidTlvMessage>(100);
        let id = Uuid::new_v4();

        // ---------------- Write-Loop -----------------------------------
        let write_jh = {
            let id_clone = id;
            tokio::spawn(async move {
                if let Err(e) = Self::write_task(writer, write_rx).await {
                    rapid_warn!("Write task error for client {}: {:?}", id_clone, e);
                }
            })
        };

        // ---------------- Reader-Loop -----------------------------------
        let read_jh = if let Some(app) = to_app {
            let id_clone = id;
            tokio::spawn(async move {
                if let Err(e) = Self::read_task(reader, app, id_clone).await {
                    rapid_warn!("Read task error for client {}: {:?}", id_clone, e);
                }
            })
        } else {
            tokio::spawn(async {})   // leeres Future -> JoinHandle<()>
        };

        Self { id, addr, write_tx, read_jh, write_jh }
    }

    /* ----------------------------------------------------------------
       Öffentliche Helfer
    ---------------------------------------------------------------- */
    pub async fn send(&self, msg: RapidTlvMessage) -> Result<(), ClientError> {
        self.write_tx
            .send(msg)
            .await
            .map_err(|_| ClientError::ChannelClosed)
    }

    pub fn is_alive(&self) -> bool {
        !self.write_jh.is_finished() && !self.read_jh.is_finished()
    }

    pub async fn shutdown(self) {
        self.write_jh.abort();
        self.read_jh.abort();
    }

    /* ----------------------------------------------------------------
       PRIVATE Loops – unverändert aus bisherigem Code gekürzt
    ---------------------------------------------------------------- */
    async fn read_task(
        mut reader: OwnedReadHalf,
        to_app_tx:  Sender<ClientEvent>,
        client_id: Uuid,
    ) -> Result<(), ClientError> {
        rapid_info!("Read task started for client {}", client_id);

        // Pre-allocate a reusable buffer with a reasonable size
        let mut read_buffer = BytesMut::with_capacity(4);
        let mut len_buf = [0u8; 4];
        let mut len_bytes_read = 0;

        loop {
            // Read message length (handling partial reads)
            while len_bytes_read < 4 {
                rapid_debug!("Client {} reading message length, {} bytes read so far", client_id, len_bytes_read);

                match reader.read(&mut len_buf[len_bytes_read..]).await {
                    Ok(n) => {
                        if n == 0 {
                            // Connection closed
                            rapid_debug!("Connection closed by peer for client {}", client_id);
                            return Err(ClientError::ConnectionClosed);
                        }
                        len_bytes_read += n;
                        rapid_debug!("Client {} read {} bytes of length, total: {}/4", client_id, n, len_bytes_read);
                    },
                    Err(e) => {
                        return if e.kind() == std::io::ErrorKind::UnexpectedEof {
                            rapid_debug!("Connection closed by peer for client {}", client_id);
                            Err(ClientError::ConnectionClosed)
                        } else {
                            rapid_error!("Error reading message length for client {}: {:?}", client_id, e);
                            Err(ClientError::Io(e))
                        }
                    }
                }
            }

            // Reset for the next message
            len_bytes_read = 0;

            let len = u32::from_be_bytes(len_buf) as usize;
            rapid_info!("Client {} received message with length: {}", client_id, len);

            if len > 10 * 1024 * 1024 {
                rapid_error!("Message too large for client {}: {} bytes", client_id, len);
                return Err(ClientError::MessageTooLarge(len));
            }

            // Ensure the buffer has enough capacity
            if read_buffer.capacity() < len {
                read_buffer.reserve(len - read_buffer.capacity());
            }

            // Clear buffer but keep allocated memory
            read_buffer.clear();

            // Add length bytes to the buffer
            read_buffer.extend_from_slice(&len_buf);

            // Prepare to read the rest of the message
            let remaining = len - 4;
            rapid_debug!("Client {} reading remaining {} bytes", client_id, remaining);

            // Ensure we have space for the remaining bytes
            read_buffer.resize(len, 0);

            // Read the rest of the message directly into the buffer (handling partial reads)
            let mut bytes_read = 0;
            while bytes_read < remaining {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    reader.read(&mut read_buffer[4 + bytes_read..len])
                ).await {
                    Ok(Ok(n)) => {
                        if n == 0 {
                            // Connection closed
                            rapid_error!("Connection closed by peer while reading message body for client {}", client_id);
                            return Err(ClientError::ConnectionClosed);
                        }
                        bytes_read += n;
                        rapid_debug!("Client {} read {} bytes of message body, total: {}/{}", client_id, n, bytes_read, remaining);
                    },
                    Ok(Err(e)) => {
                        rapid_error!("Error reading message data for client {}: {:?}", client_id, e);
                        return Err(ClientError::Io(e));
                    },
                    Err(_) => {
                        rapid_error!("Timeout reading message data for client {}", client_id);
                        return Err(ClientError::ReadTimeout);
                    }
                }
            }

            rapid_debug!("Client {} read message data successfully", client_id);

            // Parse the message directly from our buffer
            match RapidTlvMessage::parse(read_buffer.freeze()) {
                Ok(msg) => {
                    rapid_info!("Client {} parsed message successfully", client_id);
                    let client_msg = ClientEvent::Message {
                        client_id,
                        message: msg,
                    };

                    // Use blocking sending to ensure a message is delivered
                    match to_app_tx.send(client_msg).await {
                        Ok(_) => {
                            rapid_info!("Client {} forwarded message to application", client_id);
                        },
                        Err(e) => {
                            rapid_error!("Failed to forward message for client {}: {:?}", client_id, e);
                            return Err(ClientError::ChannelClosed);
                        }
                    }

                    // Create a new BytesMut for the next message
                    read_buffer = BytesMut::with_capacity(16 * 1024);
                },
                Err(e) => {
                    rapid_warn!("Error parsing message for client {}: {:?}", client_id, e);

                    // Create a new BytesMut for the next message
                    read_buffer = BytesMut::with_capacity(16 * 1024);
                }
            }
        }
    }

    async fn write_task(
        mut writer: OwnedWriteHalf,
        mut rx:     Receiver<RapidTlvMessage>,
    ) -> Result<(), ClientError> {
        rapid_debug!("Write task waiting for messages");

        // Track when we last flushed to avoid excessive flushes
        let mut last_flush = std::time::Instant::now();
        // Flush interval in milliseconds
        const FLUSH_INTERVAL_MS: u128 = 50; // Flush at most every 50 ms

        // Try to batch messages if they arrive quickly
        let mut pending_messages = Vec::with_capacity(10);

        loop {
            // Use timeout_recv to implement batching with a small delay
            match tokio::time::timeout(
                std::time::Duration::from_millis(5), // Small delay to allow batching
                rx.recv()
            ).await {
                // Got a message within timeout
                Ok(Some(mut msg)) => {
                    rapid_debug!("Write task received message");

                    match msg.encode() {
                        Ok(data) => {
                            // Store the encoded message for batched writing
                            pending_messages.push(data.to_vec());

                            // Process any additional pending messages without delay
                            while let Ok(mut next_msg) = rx.try_recv() {
                                if let Ok(next_data) = next_msg.encode() {
                                    pending_messages.push(next_data.to_vec());

                                    // Limit batch size to avoid excessive memory usage
                                    if pending_messages.len() >= 10 {
                                        break;
                                    }
                                }
                            }

                            // Write all pending messages
                            for data in &pending_messages {
                                match tokio::time::timeout(
                                    std::time::Duration::from_secs(5),
                                    writer.write_all(data)
                                ).await {
                                    Ok(Ok(_)) => {},
                                    Ok(Err(e)) => {
                                        rapid_error!("Error writing message data: {:?}", e);
                                        return Err(ClientError::Io(e));
                                    },
                                    Err(_) => {
                                        rapid_error!("Timeout writing message data");
                                        return Err(ClientError::WriteTimeout);
                                    }
                                }
                            }

                            // Only flush if enough time has passed since the last flush
                            let now = std::time::Instant::now();
                            if now.duration_since(last_flush).as_millis() >= FLUSH_INTERVAL_MS {
                                match tokio::time::timeout(
                                    std::time::Duration::from_secs(5),
                                    writer.flush()
                                ).await {
                                    Ok(Ok(_)) => {
                                        last_flush = now;
                                        rapid_debug!("Flushed writer successfully");
                                    },
                                    Ok(Err(e)) => {
                                        rapid_error!("Error flushing writer: {:?}", e);
                                        return Err(ClientError::Io(e));
                                    },
                                    Err(_) => {
                                        rapid_error!("Timeout flushing writer");
                                        return Err(ClientError::FlushTimeout);
                                    }
                                }
                            }

                            // Clear pending messages
                            pending_messages.clear();
                        },
                        Err(e) => {
                            rapid_warn!("Error encoding message: {:?}", e);
                            // Continue processing other messages even if one fails to encode
                        }
                    }
                },
                // Channel closed
                Ok(None) => {
                    rapid_debug!("Write channel closed, shutting down write task");
                    return Ok(());
                },
                // Timeout with no message - check if we need to flush
                Err(_) => {
                    // If we have pending messages, write and flush them
                    if !pending_messages.is_empty() {
                        for data in &pending_messages {
                            if let Err(_) = tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                writer.write_all(data)
                            ).await {
                                rapid_error!("Timeout writing message data");
                                return Err(ClientError::WriteTimeout);
                            }
                        }

                        // Flush after writing all pending messages
                        if let Err(_) = tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            writer.flush()
                        ).await {
                            rapid_error!("Timeout flushing writer");
                            return Err(ClientError::FlushTimeout);
                        }

                        last_flush = std::time::Instant::now();
                        pending_messages.clear();
                    }
                }
            }
        }
    }
}