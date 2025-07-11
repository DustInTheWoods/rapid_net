# RapidNet

[![Crates.io](https://img.shields.io/crates/v/rapid_net.svg)](https://crates.io/crates/rapid_net)
[![Documentation](https://docs.rs/rapid_net/badge.svg)](https://docs.rs/rapid_net)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/rapid_net.svg)](LICENSE)

A high-performance, asynchronous networking library for Rust, built on top of Tokio. It provides a simple API for building client-server applications with TLV (Type-Length-Value) message encoding.

## Features

- Asynchronous client and server implementations
- Support for both TCP and Unix socket connections
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
    let server_cfg = RapidServerConfig::new(addr.clone(), true, None); // Using TCP by default
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
        let client_cfg = RapidClientConfig::new(addr.clone(), true, None); // Using TCP by default
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

## Socket Type Configuration

RapidNet supports both TCP and Unix socket connections. You can configure the socket type when creating the server or client configuration.

### TCP Socket (Default)

TCP sockets are the default and work across network boundaries. To configure a TCP socket:

1. Import the necessary types: `RapidServerConfig`, `RapidClientConfig`, and `SocketType` from `rapid_net::config`.
2. Create a server configuration with TCP socket by calling `RapidServerConfig::new()` with:
   - The address as a string (e.g., "127.0.0.1:9000")
   - The no_delay flag (true/false)
   - The socket type as `Some(SocketType::Tcp)` or `None` for default TCP

3. Create a client configuration with TCP socket similarly using `RapidClientConfig::new()`.

### Unix Socket

Unix sockets provide faster communication on the same machine. To configure a Unix socket:

1. Import the necessary types: `RapidServerConfig`, `RapidClientConfig`, and `SocketType` from `rapid_net::config`.
2. Create a server configuration with Unix socket by calling `RapidServerConfig::new()` with:
   - The socket path as a string (e.g., "/tmp/rapid_net.sock")
   - The no_delay flag (true/false)
   - The socket type as `Some(SocketType::Unix)`

3. Create a client configuration with Unix socket similarly using `RapidClientConfig::new()`.

Note: Unix sockets are only available on Unix-like operating systems (Linux, macOS, etc.) and not on Windows.

## Configuration Options

### RapidServerConfig

- `address`: The address to bind to (IP:port for TCP, file path for Unix socket)
- `no_delay`: Whether to enable TCP_NODELAY (reduces latency but may increase bandwidth usage)
- `socket_type`: The type of socket to use (TCP or Unix)

### RapidClientConfig

- `address`: The address to connect to (IP:port for TCP, file path for Unix socket)
- `no_delay`: Whether to enable TCP_NODELAY (reduces latency but may increase bandwidth usage)
- `socket_type`: The type of socket to use (TCP or Unix)

## Usage Patterns

### Basic Server-Client Communication

To set up basic server-client communication:

1. **Server Setup**:
   - Create a server configuration with `RapidServerConfig::new()`
   - Create a channel for server events with `mpsc::channel::<ServerEvent>()`
   - Create a server instance with `RapidServer::new()`
   - Run the server in a separate task with `server.run(server_tx).await`

2. **Client Setup**:
   - Create a client configuration with `RapidClientConfig::new()`
   - Connect the client with `RapidClient::connect()`
   - Send messages with `client.send_message()`

3. **Message Handling**:
   - Create messages with `RapidTlvMessage::new()`
   - Add fields with `msg.add_field()`
   - Parse received messages by accessing fields with `message.get_field()`

### Using Unix Sockets

For faster local communication on Unix-like systems:

1. **Server Setup with Unix Socket**:
   - Create a server configuration with Unix socket type
   - Specify a file path for the socket (e.g., "/tmp/rapid_net.sock")
   - Set the socket type to `SocketType::Unix`

2. **Client Setup with Unix Socket**:
   - Create a client configuration with the same socket path
   - Set the socket type to `SocketType::Unix`
   - Connect and use the client as with TCP sockets

### Handling Server Events

Process server events in a loop:

1. Receive events with `server_rx.recv().await`
2. Match on event types:
   - `ServerEvent::Connected`: Handle new client connections
   - `ServerEvent::Message`: Process incoming messages
   - `ServerEvent::Disconnected`: Handle client disconnections
   - `ServerEvent::Error`: Handle client errors

3. Send responses to clients with `server.send_to_client()`

## Documentation

For more detailed documentation, please see the [API documentation](https://docs.rs/rapid_net).

## Architecture

RapidNet uses an event-based architecture:

1. **Server**: Listens for incoming connections (TCP or Unix socket) and spawns a task for each client
2. **Client**: Connects to a server (TCP or Unix socket) and provides methods for sending messages
3. **Configuration**: Allows selecting between TCP and Unix socket connections
4. **Events**: Both client and server communicate via event channels
5. **Messages**: Uses the TLV (Type-Length-Value) format for efficient binary communication

## Performance Considerations

- The library uses Tokio's asynchronous I/O for high performance
- TCP_NODELAY is enabled by default for lower latency
- Unix sockets provide faster communication for local processes
- Message batching is implemented for higher throughput
- Buffer reuse minimizes memory allocations
- Large messages are automatically chunked to prevent stack overflow

## Testing Unix Sockets on Windows

Since Unix sockets are not natively supported on Windows, we provide a Docker-based solution for testing Unix socket functionality on Windows systems:

### Prerequisites

- [Docker Desktop for Windows](https://www.docker.com/products/docker-desktop/) (or Docker for Linux/macOS)
- Git (to clone the repository)

### Verifying Docker Setup

Before running the tests, you can verify that your Docker setup is working correctly:

**Windows:**
```
.\test_docker_setup.bat
```

**Linux/macOS:**
```
chmod +x test_docker_setup.sh
./test_docker_setup.sh
```

### Running Unix Socket Tests

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/rapid_net.git
   cd rapid_net
   ```

2. Run the tests using the provided script:

   **Windows:**
   ```
   .\run_unix_tests.bat
   ```

   **Linux/macOS:**
   ```
   chmod +x run_unix_tests.sh
   ./run_unix_tests.sh
   ```

   Or, if you prefer to use Docker Compose directly:
   ```
   docker-compose up --build
   ```

### What the Docker Setup Does

The Docker setup:

1. Creates a Linux environment with Rust installed
2. Mounts your project directory into the container
3. Runs the Unix socket tests in the Linux environment
4. Displays the test results

### Test Results

The tests will verify:

1. Basic Unix socket server initialization
2. Client-server communication over Unix sockets
3. Multiple client connections using Unix sockets
4. Performance comparison between Unix sockets and TCP sockets

The performance comparison test will show how much faster Unix sockets are compared to TCP sockets for local communication, which can be significant for high-throughput applications.

## Development Container

For a consistent development environment, this project includes a Visual Studio Code Dev Container configuration. This allows you to develop inside a Docker container with all the necessary tools and extensions pre-configured.

### Prerequisites

1. [Docker Desktop](https://www.docker.com/products/docker-desktop/) installed and running
2. [Visual Studio Code](https://code.visualstudio.com/) installed
3. [Remote - Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) installed in VS Code

### Getting Started with Dev Container

1. Open this repository in Visual Studio Code
2. When prompted, click "Reopen in Container" or run the "Remote-Containers: Reopen in Container" command from the command palette
3. VS Code will build the container and connect to it, which may take a few minutes the first time
4. Once connected, you'll have a full Rust development environment with:
   - Rust Analyzer for code intelligence
   - Debugging support via LLDB
   - Cargo integration
   - Code formatting and linting tools

For more details, see the [.devcontainer/README.md](.devcontainer/README.md) file.

## License

This project is licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
