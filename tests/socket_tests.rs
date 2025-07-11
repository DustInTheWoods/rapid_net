use std::{collections::HashSet, sync::Arc, time::Duration};

use bytes::Bytes;
use rapid_net::{
    config::{RapidClientConfig, RapidServerConfig, SocketType},
    message::ServerEvent,
    RapidClient, RapidServer,
};
use rapid_tlv::RapidTlvMessage;

use tokio::{sync::mpsc, time::sleep};
use uuid::Uuid;

// Constants for test messages
const EVT_MSG: u8 = 0x10;
const FLD_KEY: u8 = 0x10;
const FLD_VAL: u8 = 0x11;

// Helper function to create a TLV message
fn make_msg(key: &str, val: &str) -> RapidTlvMessage {
    let mut msg = RapidTlvMessage::new(EVT_MSG.into());
    if !key.is_empty() {
        msg.add_field(FLD_KEY.into(), Bytes::from(key.to_owned()));
    }
    if !val.is_empty() {
        msg.add_field(FLD_VAL.into(), Bytes::from(val.to_owned()));
    }
    msg
}

/* ------------------------------------------------------------------------ */
/* TCP Socket Tests                                                         */
/* ------------------------------------------------------------------------ */

#[tokio::test]
async fn test_tcp_server_initialization() {
    // Create a server configuration with a random port and TCP socket type
    let addr = "127.0.0.1:0".to_string();
    let cfg = RapidServerConfig::new(addr, true, Some(SocketType::Tcp));

    // Create a server instance
    let server = RapidServer::new(cfg);

    // Verify the server was created successfully
    let random_uuid = Uuid::new_v4();
    let result = Arc::new(server).send_to_client(&random_uuid, RapidTlvMessage::new(EVT_MSG)).await;

    // Check that we get a ClientNotFound error
    match result {
        Err(rapid_net::server::ServerError::ClientNotFound(uuid)) => {
            assert_eq!(uuid, random_uuid);
        },
        _ => panic!("Expected ClientNotFound error, got {:?}", result),
    };
}

#[tokio::test]
async fn test_tcp_client_server_communication() {
    // Start server with TCP socket
    let port = 19_878;
    let addr_str = format!("127.0.0.1:{port}");
    let srv_cfg = RapidServerConfig::new(addr_str.clone(), true, Some(SocketType::Tcp));
    let (srv_tx, mut srv_rx) = mpsc::channel::<ServerEvent>(100);
    let server = Arc::new(RapidServer::new(srv_cfg));
    let _srv_handle = tokio::spawn({
        let s = server.clone();
        async move { s.run(srv_tx).await }
    });
    sleep(Duration::from_millis(150)).await; // Wait for server to start

    // Connect client with TCP socket
    let cli_cfg = RapidClientConfig::new(addr_str.clone(), true, Some(SocketType::Tcp));
    let mut client = RapidClient::connect(&cli_cfg).await
        .expect("connect");

    // Verify server received connection
    let ev = tokio::time::timeout(Duration::from_secs(2), srv_rx.recv())
        .await
        .expect("srv ev timeout")
        .expect("no event");
    assert!(matches!(ev, ServerEvent::Connected{..}));

    // Send message from client to server
    client.send(make_msg("TcpKey", "TcpVal")).await.unwrap();

    // Verify server received message
    let msg_ev = tokio::time::timeout(Duration::from_secs(2), srv_rx.recv())
        .await
        .expect("msg timeout")
        .expect("no event");
    let (cli_id, tlv) = match msg_ev {
        ServerEvent::Message{client_id, message} => (client_id, message),
        other => panic!("unexpected server event {other:?}"),
    };
    assert_eq!(
        tlv.get_field(&FLD_KEY.into()).unwrap().value(),
        b"TcpKey"
    );

    // Send response from server to client
    server
        .send_to_client(&cli_id, make_msg("", "TcpResp"))
        .await
        .unwrap();

    // Verify client received response
    sleep(Duration::from_millis(100)).await;
    let ans = client.recv().await.expect("no response");
    assert_eq!(
        ans.get_field(&FLD_VAL.into()).unwrap().value(),
        b"TcpResp"
    );
}

#[tokio::test]
async fn test_tcp_multiple_clients() {
    // Start server with TCP socket
    let port = 19_879;
    let addr_str = format!("127.0.0.1:{port}");
    let cfg_srv = RapidServerConfig::new(addr_str.clone(), true, Some(SocketType::Tcp));
    let (tx_srv, mut rx_srv) = mpsc::channel::<ServerEvent>(200);
    let srv = Arc::new(RapidServer::new(cfg_srv));
    tokio::spawn({
        let s = srv.clone();
        async move { s.run(tx_srv).await }
    });
    sleep(Duration::from_millis(120)).await;

    // Connect multiple clients with TCP socket
    let mut clients = Vec::new();
    for _ in 0..3 {
        let cfg = RapidClientConfig::new(addr_str.clone(), true, Some(SocketType::Tcp));
        let cli = RapidClient::connect(&cfg).await.unwrap();
        clients.push(cli);
    }

    // Drain the Connected events
    while let Ok(Some(ServerEvent::Connected{..})) =
        tokio::time::timeout(Duration::from_millis(20), rx_srv.recv()).await {}

    // Each client sends a message
    for (i, cli) in clients.iter().enumerate() {
        cli.send(make_msg(&format!("tcp_k{i}"), &format!("tcp_v{i}")))
            .await
            .unwrap();
    }

    // Server receives all messages
    let mut received = HashSet::<Uuid>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while received.len() < clients.len() && tokio::time::Instant::now() < deadline {
        if let Ok(Some(ServerEvent::Message{client_id, ..})) =
            tokio::time::timeout(Duration::from_millis(200), rx_srv.recv()).await
        {
            received.insert(client_id);
        }
    }
    assert_eq!(received.len(), clients.len(), "not all TCP messages seen");
}

