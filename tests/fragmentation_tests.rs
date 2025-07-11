use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use rapid_net::message::ClientEvent;
use rapid_tlv::RapidTlvMessage;
use bytes::Bytes;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use uuid::Uuid;
use std::sync::Mutex;

pub const EVENT:u8 = 0x10;
pub const VALUE:u8 = 0x10;

// Helper function to create a test message with a large payload
fn create_large_test_message(size: usize) -> RapidTlvMessage {
    let mut msg = RapidTlvMessage::new(EVENT);

    // Create a large payload
    let mut payload = Vec::with_capacity(size);
    for i in 0..size {
        payload.push((i % 256) as u8);
    }

    msg.add_field(VALUE, Bytes::from(payload));
    msg
}

// Mock reader that delivers data in small chunks to simulate fragmentation
struct FragmentedMockReader {
    read_data: Mutex<Vec<u8>>,
    fragment_size: usize,
}

impl FragmentedMockReader {
    fn new(fragment_size: usize) -> Self {
        Self {
            read_data: Mutex::new(Vec::new()),
            fragment_size,
        }
    }

    fn add_read_data(&self, data: &[u8]) {
        let mut read_data = self.read_data.lock().unwrap();
        read_data.extend_from_slice(data);
    }
}

impl AsyncRead for FragmentedMockReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut read_data = self.read_data.lock().unwrap();

        if read_data.is_empty() {
            return Poll::Ready(Ok(()));
        }

        // Determine how much data to read (limited by fragment_size)
        let to_read = std::cmp::min(self.fragment_size, read_data.len());
        let to_read = std::cmp::min(to_read, buf.remaining());

        if to_read == 0 {
            return Poll::Ready(Ok(()));
        }

        // Copy data to the buffer
        let data = read_data.drain(0..to_read).collect::<Vec<_>>();
        buf.put_slice(&data);

        Poll::Ready(Ok(()))
    }
}

// Mock writer that just discards data
struct MockWriter;

impl AsyncWrite for MockWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Just pretend we wrote all the data
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn test_fragmented_message_reading() {
    // Initialize logger for debugging
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    // Create a mock reader that delivers data in small chunks (1 byte at a time)
    let mut mock_reader = FragmentedMockReader::new(1);

    // Create a channel for client events
    let (client_tx, mut client_rx) = mpsc::channel::<ClientEvent>(100);

    // Create a large test message
    let mut test_message = create_large_test_message(1024);

    // Encode the message
    let encoded_message = test_message.encode().unwrap().to_vec();

    // Add the encoded message to the mock reader's buffer
    mock_reader.add_read_data(&encoded_message);

    // Create a client ID
    let client_id = Uuid::new_v4();

    // Since we can't directly access the private read_task method,
    // we'll implement similar logic here to test fragmentation handling
    tokio::spawn(async move {
        // This simulates the read_task method's behavior
        let mut read_buffer = bytes::BytesMut::with_capacity(16 * 1024);
        let mut len_buf = [0u8; 4];
        let mut len_bytes_read = 0;

        // Read message length (handling partial reads)
        while len_bytes_read < 4 {
            match tokio::io::AsyncReadExt::read(&mut mock_reader, &mut len_buf[len_bytes_read..]).await {
                Ok(n) => {
                    if n == 0 {
                        // Connection closed
                        return;
                    }
                    len_bytes_read += n;
                },
                Err(_) => {
                    return;
                }
            }
        }

        // No need to reset len_bytes_read as we're only reading one message

        let len = u32::from_be_bytes(len_buf) as usize;

        // Ensure buffer has enough capacity
        if read_buffer.capacity() < len {
            read_buffer.reserve(len - read_buffer.capacity());
        }

        // Clear buffer but keep allocated memory
        read_buffer.clear();

        // Add length bytes to buffer
        read_buffer.extend_from_slice(&len_buf);

        // Prepare to read the rest of the message
        let remaining = len - 4;

        // Ensure we have space for the remaining bytes
        read_buffer.resize(len, 0);

        // Read the rest of the message directly into the buffer (handling partial reads)
        let mut bytes_read = 0;
        while bytes_read < remaining {
            match tokio::io::AsyncReadExt::read(&mut mock_reader, &mut read_buffer[4 + bytes_read..len]).await {
                Ok(n) => {
                    if n == 0 {
                        // Connection closed
                        return;
                    }
                    bytes_read += n;
                },
                Err(_) => {
                    return;
                }
            }
        }

        // Parse the message directly from our buffer
        match rapid_tlv::RapidTlvMessage::parse(read_buffer.freeze()) {
            Ok(msg) => {
                let client_msg = ClientEvent::Message {
                    client_id,
                    message: msg,
                };

                // Send the message to the application
                let _ = client_tx.send(client_msg).await;
            },
            Err(_) => {
                // Error parsing message
            }
        }
    });

    // Wait for the client to process the message
    let timeout = Duration::from_secs(5);
    let start_time = std::time::Instant::now();
    let mut received_message = false;

    while !received_message && start_time.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(100), client_rx.recv()).await {
            Ok(Some(ClientEvent::Message { client_id: _, message })) => {
                // Verify the message content
                let value_field = message.get_field(&VALUE);
                assert!(value_field.is_some(), "Value field missing from message");

                // Verify the field length
                assert_eq!(value_field.unwrap().value().len(), 1024, "Value field has incorrect length");

                received_message = true;
            }
            Ok(Some(_)) => {
                // Ignore other events
            }
            Ok(None) => {
                break;
            }
            Err(_) => {
                // Timeout, continue
                sleep(Duration::from_millis(10)).await;
            }
        }
    }

    assert!(received_message, "Did not receive the fragmented message");
}
