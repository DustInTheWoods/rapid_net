# RapidNet

[![Crates.io](https://img.shields.io/crates/v/rapid_net.svg)](https://crates.io/crates/rapid_net)
[![Documentation](https://docs.rs/rapid_net/badge.svg)](https://docs.rs/rapid_net)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/rapid_net.svg)](LICENSE)

A high-performance, asynchronous networking library for Rust, built on top of Tokio. It provides a simple API for building client-server applications with TLV (Type-Length-Value) message encoding.

## Features

- Asynchronous client and server implementations
- TLV message encoding for efficient binary communication
- Event-based architecture for handling connections, messages, and errors
- Automatic reconnection handling
- Configurable performance options

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rapid_net = "0.1.0"
rapid_tlv = "0.1.0"  # Required dependency
```

## Quick Start

Here's a simple example of a client-server application:

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio::task;
use rapid_net::config::{RapidServerConfig, RapidClientConfig};
use rapid_net::server::RapidServer;
use rapid_net::client::RapidClient;
use rapid_net::message::{ClientEvent, ServerEvent};
use rapid_tlv::{RapidTlvMessage, RapidTlvEventType, RapidTlvFieldType};
use bytes::Bytes;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    let addr = "127.0.0.1:9000".to_string();

    // Start server
    let server_cfg = RapidServerConfig::new(addr.clone(), true);
    let (server_tx, mut server_rx) = mpsc::channel::<ServerEvent>(100);
    let server = Arc::new(RapidServer::new(server_cfg));

    let server_handle = {
        let server = server.clone();
        task::spawn(async move {
            server.run(server_tx).await;
        })
    };

    // Start client after a short delay (to allow server to bind)
    sleep(Duration::from_millis(100)).await;

    let client_handle = task::spawn(async move {
        // 1. Create client event channel
        let (client_tx, mut client_rx) = mpsc::channel::<ClientEvent>(100);

        // 2. Configure and connect client
        let client_cfg = RapidClientConfig::new(addr.clone(), true);
        let client = RapidClient::new(client_cfg, client_tx).await.expect("Client connect failed");

        println!("Client {} connected to {}", client.id(), client.addr());

        // 3. Send message to server
        let mut msg = RapidTlvMessage::new(RapidTlvEventType::Event);
        msg.add_field(RapidTlvFieldType::Key, Bytes::from("Hello from client"));
        client.send_message(msg).await.expect("Send failed");

        // 4. Receive incoming messages (optional)
        if let Some(ClientEvent::Message { message, .. }) = client_rx.recv().await {
            if let Some(f) = message.get_field(&RapidTlvFieldType::Value) {
                println!("Response from server: {}", String::from_utf8_lossy(f.value()));
            }
        }

        client.close().await;
    });

    // 5. Process server events
    let server_logic = task::spawn(async move {
        while let Some(event) = server_rx.recv().await {
            match event {
                ServerEvent::Connected { client } => {
                    println!("Client connected: {}", client.id());
                }
                ServerEvent::Message { client, message } => {
                    println!("Message received from {}", client.id());

                    if let Some(field) = message.get_field(&RapidTlvFieldType::Key) {
                        println!("Content: {}", String::from_utf8_lossy(field.value()));

                        // Send response
                        let mut response = RapidTlvMessage::new(RapidTlvEventType::Event);
                        response.add_field(RapidTlvFieldType::Value, Bytes::from("Hello from server"));

                        client.send_message(response).await.expect("Response failed");
                    }
                }
                ServerEvent::Disconnected { client } => {
                    println!("Client disconnected: {}", client.id());
                }
                ServerEvent::Error { client, error } => {
                    eprintln!("Error from {}: {}", client.id(), error);
                }
            }
        }
    });

    let _ = tokio::join!(server_handle, client_handle, server_logic);
}
```

## Documentation

For more detailed documentation, please see the [API documentation](https://docs.rs/rapid_net).

## Architecture

RapidNet uses an event-based architecture:

1. **Server**: Listens for incoming connections and spawns a task for each client
2. **Client**: Connects to a server and provides methods for sending messages
3. **Events**: Both client and server communicate via event channels
4. **Messages**: Uses the TLV (Type-Length-Value) format for efficient binary communication

## Performance Considerations

- The library uses Tokio's asynchronous I/O for high performance
- TCP_NODELAY is enabled by default for lower latency
- Message batching is implemented for higher throughput
- Buffer reuse minimizes memory allocations

## License

This project is licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.