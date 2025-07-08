use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;
use rapid_net::config::RapidServerConfig;
use rapid_net::server::{RapidServer, ServerError};
use rapid_net::message::ServerEvent;
use rapid_tlv::{RapidTlvMessage, RapidTlvEventType};

pub const MSG_EVENT: u8 = 0x01;

#[tokio::test]
async fn test_server_initialization() {
    // Create a server configuration with a random port
    let addr = "127.0.0.1:0".to_string();
    let cfg = RapidServerConfig::new(addr, true);

    // Create a server instance
    let server = RapidServer::new(cfg);

    // Just verify the server was created successfully
    // We can't directly access private fields, so we'll just check that the instance exists
    let random_uuid = Uuid::new_v4();
    let result = Arc::new(server).send_to_client(&random_uuid, RapidTlvMessage::new(MSG_EVENT)).await;

    // Check that we get a ClientNotFound error
    match result {
        Err(ServerError::ClientNotFound(uuid)) => {
            assert_eq!(uuid, random_uuid);
        },
        _ => panic!("Expected ClientNotFound error, got {:?}", result),
    };
}

#[tokio::test]
async fn test_server_client_management() {
    // This test would normally require mocking TcpListener and TcpStream
    // For simplicity, we'll just test the public API

    let addr = "127.0.0.1:0".to_string();
    let cfg = RapidServerConfig::new(addr, true);
    let server = Arc::new(RapidServer::new(cfg));

    // Create a channel for server events
    let (tx, _rx) = mpsc::channel::<ServerEvent>(100);

    // Spawn the server in a separate task
    let server_clone = server.clone();
    tokio::spawn(async move {
        server_clone.run(tx).await;
    });

    // Wait a bit for the server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Since we can't easily test add_client and remove_client directly (they're private),
    // we'll test send_to_client which indirectly tests client management
    let random_uuid = Uuid::new_v4();
    let result = server.send_to_client(&random_uuid, RapidTlvMessage::new(MSG_EVENT)).await;

    // Should fail since no client with this UUID exists
    match result {
        Err(ServerError::ClientNotFound(uuid)) => {
            assert_eq!(uuid, random_uuid);
        },
        _ => panic!("Expected ClientNotFound error, got {:?}", result),
    };
}



// In a real test environment, we would use mocks to test more functionality
// For example, mocking TcpListener to test connection acceptance
// and mocking RapidClient to test client_task and message handling
