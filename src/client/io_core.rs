use crate::{rapid_debug, rapid_error, rapid_warn, ClientEvent};
use bytes::BytesMut;
use rapid_tlv::RapidTlvMessage;
use std::net::SocketAddr as StdSocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::net::tcp::{OwnedReadHalf as TcpReadHalf, OwnedWriteHalf as TcpWriteHalf};
#[cfg(unix)]
use tokio::net::unix::{OwnedReadHalf as UnixReadHalf, OwnedWriteHalf as UnixWriteHalf};
use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;
use crate::client::error::ClientError;
#[cfg(unix)]
use std::path::Path;

// Enum to represent different types of socket addresses
#[derive(Clone, Debug)]
pub enum SocketAddr {
    Tcp(StdSocketAddr),
    #[cfg(unix)]
    Unix(String),
    #[cfg(not(unix))]
    UnixUnsupported(String), // Placeholder for non-Unix platforms
}

impl std::fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketAddr::Tcp(addr) => write!(f, "{}", addr),
            #[cfg(unix)]
            SocketAddr::Unix(path) => write!(f, "unix:{}", path),
            #[cfg(not(unix))]
            SocketAddr::UnixUnsupported(path) => write!(f, "unix:{} (unsupported)", path),
        }
    }
}

// Enum to represent different types of read halves
enum ReadHalf {
    Tcp(TcpReadHalf),
    #[cfg(unix)]
    Unix(UnixReadHalf),
}

// Enum to represent different types of write halves
enum WriteHalf {
    Tcp(TcpWriteHalf),
    #[cfg(unix)]
    Unix(UnixWriteHalf),
}

// Implement AsyncRead for ReadHalf
impl AsyncRead for ReadHalf {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ReadHalf::Tcp(tcp) => std::pin::Pin::new(tcp).poll_read(cx, buf),
            #[cfg(unix)]
            ReadHalf::Unix(unix) => std::pin::Pin::new(unix).poll_read(cx, buf),
        }
    }
}

// Implement AsyncWrite for WriteHalf
impl AsyncWrite for WriteHalf {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            WriteHalf::Tcp(tcp) => std::pin::Pin::new(tcp).poll_write(cx, buf),
            #[cfg(unix)]
            WriteHalf::Unix(unix) => std::pin::Pin::new(unix).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            WriteHalf::Tcp(tcp) => std::pin::Pin::new(tcp).poll_flush(cx),
            #[cfg(unix)]
            WriteHalf::Unix(unix) => std::pin::Pin::new(unix).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            WriteHalf::Tcp(tcp) => std::pin::Pin::new(tcp).poll_shutdown(cx),
            #[cfg(unix)]
            WriteHalf::Unix(unix) => std::pin::Pin::new(unix).poll_shutdown(cx),
        }
    }
}

pub(crate) struct IoCore {
    pub id:   Uuid,
    pub addr: SocketAddr,
    write_tx: Sender<RapidTlvMessage>,
    read_jh:  tokio::task::JoinHandle<()>,
    write_jh: tokio::task::JoinHandle<()>,
}

impl IoCore {
    pub fn new_tcp(
        stream: TcpStream,
        addr: std::net::SocketAddr,
        to_app: Option<Sender<ClientEvent>>, // Only inbound needs this
    ) -> Self {
        let (reader, writer) = stream.into_split();
        let reader = ReadHalf::Tcp(reader);
        let writer = WriteHalf::Tcp(writer);
        let socket_addr = SocketAddr::Tcp(addr);

        Self::new_internal(reader, writer, socket_addr, to_app)
    }

    #[cfg(unix)]
    #[allow(dead_code)]
    pub fn new_unix(
        stream: UnixStream,
        path: String,
        to_app: Option<Sender<ClientEvent>>, // Only inbound needs this
    ) -> Self {
        let (reader, writer) = stream.into_split();
        let reader = ReadHalf::Unix(reader);
        let writer = WriteHalf::Unix(writer);
        let socket_addr = SocketAddr::Unix(path);

        Self::new_internal(reader, writer, socket_addr, to_app)
    }

    #[cfg(not(unix))]
    pub fn new_unix(
        _stream: std::io::Error, // We use Error as a placeholder since UnixStream doesn't exist on non-Unix
        path: String,
        _to_app: Option<Sender<ClientEvent>>,
    ) -> Self {
        panic!("Unix sockets are not supported on this platform: {}", path);
    }

    fn new_internal(
        reader: ReadHalf,
        writer: WriteHalf,
        addr: SocketAddr,
        to_app: Option<Sender<ClientEvent>>,
    ) -> Self {
        let (write_tx, write_rx) = mpsc::channel::<RapidTlvMessage>(100);
        let id = Uuid::new_v4();

        // Create separate tasks for reading and writing
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
            tokio::spawn(async {})   // Empty future -> JoinHandle<()>
        };

        Self { id, addr, write_tx, read_jh, write_jh }
    }

    /* ----------------------------------------------------------------
       Public Helpers
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


    /* ----------------------------------------------------------------
       PRIVATE Loops - shortened from previous code without changes
    ---------------------------------------------------------------- */
    async fn read_task(
        mut reader: ReadHalf,
        to_app_tx:  Sender<ClientEvent>,
        client_id: Uuid,
    ) -> Result<(), ClientError> {
        rapid_debug!("Read task started for client {}", client_id);

        // Pre-allocate a reusable buffer with a reasonable size (16KB is a good balance)
        let mut read_buffer = BytesMut::with_capacity(16 * 1024);
        let mut len_buf = [0u8; 4];
        let mut len_bytes_read = 0;

        loop {
            // Read message length (handling partial reads)
            while len_bytes_read < 4 {
                match reader.read(&mut len_buf[len_bytes_read..]).await {
                    Ok(n) => {
                        if n == 0 {
                            // Connection closed
                            rapid_debug!("Connection closed by peer for client {}", client_id);
                            return Err(ClientError::ConnectionClosed);
                        }
                        len_bytes_read += n;
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
            rapid_debug!("Server received from client {} message with length: {}", client_id, len);

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

            // Debug the first 10 bytes of the received message
            let debug_len = std::cmp::min(10, read_buffer.len());
            if debug_len > 0 {
                let debug_bytes = &read_buffer[0..debug_len];
                rapid_debug!("Server received from client {} first {} bytes: {:?}", client_id, debug_len, debug_bytes);
            }

            // Parse the message directly from our buffer
            match RapidTlvMessage::parse(read_buffer.freeze()) {
                Ok(msg) => {
                    rapid_debug!("Server parsed message from client {} successfully", client_id);
                    let client_msg = ClientEvent::Message {
                        client_id,
                        message: msg,
                    };

                    // Use blocking sending to ensure a message is delivered
                    match to_app_tx.send(client_msg).await {
                        Ok(_) => {
                            rapid_debug!("Server forwarded message from client {} to application", client_id);
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
        mut writer: WriteHalf,
        mut rx:     Receiver<RapidTlvMessage>,
    ) -> Result<(), ClientError> {
        rapid_debug!("Write task waiting for messages");

        // Track when we last flushed to avoid excessive flushes
        let mut last_flush = std::time::Instant::now();
        // Flush interval in milliseconds
        const FLUSH_INTERVAL_MS: u128 = 50; // Flush at most every 50 ms
        // Maximum chunk size for large messages to prevent stack overflow
        const MAX_CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

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
                    match msg.encode() {
                        Ok(data) => {
                            // Store the encoded message for batched writing
                            let data_vec = data.to_vec();

                            // Check if this is a large message that needs chunking
                            if data_vec.len() > MAX_CHUNK_SIZE {
                                if let Err(e) = Self::write_large_message_in_chunks(
                                    &mut writer, 
                                    &data_vec, 
                                    MAX_CHUNK_SIZE, 
                                    &mut last_flush
                                ).await {
                                    return Err(e);
                                }
                            } else {
                                // For normal-sized messages, add to batch
                                pending_messages.push(data_vec);
                            }

                            // Process any additional pending messages without delay
                            while let Ok(mut next_msg) = rx.try_recv() {
                                if let Ok(next_data) = next_msg.encode() {
                                    let next_data_vec = next_data.to_vec();

                                    // Check if this is a large message that needs immediate processing
                                    if next_data_vec.len() > MAX_CHUNK_SIZE {
                                        // First, write and flush any pending messages
                                        if !pending_messages.is_empty() {
                                            for data in &pending_messages {
                                                if let Err(e) = Self::write_with_timeout(&mut writer, data).await {
                                                    return Err(e);
                                                }
                                            }

                                            if let Err(e) = Self::flush_with_timeout(&mut writer).await {
                                                return Err(e);
                                            }

                                            last_flush = std::time::Instant::now();
                                            pending_messages.clear();
                                        }

                                        // Now process the large message in chunks
                                        if let Err(e) = Self::write_large_message_in_chunks(
                                            &mut writer, 
                                            &next_data_vec, 
                                            MAX_CHUNK_SIZE, 
                                            &mut last_flush
                                        ).await {
                                            return Err(e);
                                        }
                                    } else {
                                        // For normal-sized messages, add to batch
                                        pending_messages.push(next_data_vec);
                                    }

                                    // Limit batch size to avoid excessive memory usage
                                    if pending_messages.len() >= 10 {
                                        break;
                                    }
                                }
                            }

                            // Write all pending normal-sized messages
                            for data in &pending_messages {
                                if let Err(e) = Self::write_with_timeout(&mut writer, data).await {
                                    return Err(e);
                                }
                            }

                            // Only flush if enough time has passed since the last flush
                            let now = std::time::Instant::now();
                            if now.duration_since(last_flush).as_millis() >= FLUSH_INTERVAL_MS {
                                if let Err(e) = Self::flush_with_timeout(&mut writer).await {
                                    return Err(e);
                                }
                                last_flush = now;
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
                            if let Err(e) = Self::write_with_timeout(&mut writer, data).await {
                                return Err(e);
                            }
                        }

                        // Flush after writing all pending messages
                        if let Err(e) = Self::flush_with_timeout(&mut writer).await {
                            return Err(e);
                        }

                        last_flush = std::time::Instant::now();
                        pending_messages.clear();
                    }
                }
            }
        }
    }

    // Helper method to write data with timeout
    async fn write_with_timeout(writer: &mut WriteHalf, data: &[u8]) -> Result<(), ClientError> {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            writer.write_all(data)
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                rapid_error!("Error writing message data: {:?}", e);
                Err(ClientError::Io(e))
            },
            Err(_) => {
                rapid_error!("Timeout writing message data");
                Err(ClientError::WriteTimeout)
            }
        }
    }

    // Helper method to flush with timeout
    async fn flush_with_timeout(writer: &mut WriteHalf) -> Result<(), ClientError> {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            writer.flush()
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                rapid_error!("Error flushing writer: {:?}", e);
                Err(ClientError::Io(e))
            },
            Err(_) => {
                rapid_error!("Timeout flushing writer");
                Err(ClientError::FlushTimeout)
            }
        }
    }

    // Helper method to write large message in chunks
    async fn write_large_message_in_chunks(
        writer: &mut WriteHalf,
        data: &[u8],
        chunk_size: usize,
        last_flush: &mut std::time::Instant
    ) -> Result<(), ClientError> {
        rapid_debug!("Large message detected ({} bytes), chunking into {}KB pieces", 
                     data.len(), chunk_size / 1024);

        for chunk in data.chunks(chunk_size) {
            if let Err(e) = Self::write_with_timeout(writer, chunk).await {
                return Err(e);
            }

            // Flush after each chunk for large messages
            if let Err(e) = Self::flush_with_timeout(writer).await {
                return Err(e);
            }

            *last_flush = std::time::Instant::now();
        }

        Ok(())
    }
}
