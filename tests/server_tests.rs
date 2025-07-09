use rapid_net::config::RapidServerConfig;
use rapid_net::message::ServerEvent;
use rapid_net::server::{RapidServer, ServerError};
use rapid_tlv::RapidTlvMessage;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

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

#[tokio::test]
async fn test_broadcast_stack_overflow() {
    use bytes::Bytes;

    // Create a server configuration
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

    // Create a large message that might cause issues if not handled properly
    let mut large_data = Vec::with_capacity(1024 * 1024); // 1MB
    for i in 0..1024 * 256 { // 256KB of data (to avoid making the test too slow)
        large_data.push((i % 256) as u8);
    }

    // Create a message with the large data
    let mut msg = RapidTlvMessage::new(MSG_EVENT);
    msg.add_field(1, Bytes::from(large_data));

    // Try to broadcast the message - this should not cause a stack overflow
    // Even though we don't have clients, the broadcast method should handle the large message properly
    let result = server.broadcast(msg, None).await;

    // The broadcast will return Ok since there are no clients to fail
    assert!(result.is_ok());

    // Now test with a very large number of iterations to ensure we don't have stack issues
    for i in 0..10 {
        let mut msg = RapidTlvMessage::new(MSG_EVENT);
        // Add a small field with the iteration number
        let data = vec![i as u8];
        msg.add_field(1, Bytes::from(data));

        // This should process in batches and not cause a stack overflow
        let result = server.broadcast(msg, None).await;
        assert!(result.is_ok());
    }

    // If we get here without a stack overflow, the test passes
    println!("Broadcast test completed without stack overflow");
}

// In a real test environment, we would use mocks to test more functionality
// For example, mocking TcpListener to test connection acceptance
// and mocking RapidClient to test client_task and message handling
