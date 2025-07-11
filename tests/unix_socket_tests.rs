#[cfg(unix)]
mod unix_tests {
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

    #[tokio::test]
    async fn test_unix_server_initialization() {
        // Create a server configuration with a Unix socket path
        let socket_path = "/tmp/rapid_net_test.sock";
        let cfg = RapidServerConfig::new(socket_path.to_string(), true, Some(SocketType::Unix));

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
    async fn test_unix_client_server_communication() {
        // Start server with Unix socket
        let socket_path = "/tmp/rapid_net_test_comm.sock";
        let srv_cfg = RapidServerConfig::new(socket_path.to_string(), true, Some(SocketType::Unix));
        let (srv_tx, mut srv_rx) = mpsc::channel::<ServerEvent>(100);
        let server = Arc::new(RapidServer::new(srv_cfg));
        let _srv_handle = tokio::spawn({
            let s = server.clone();
            async move { s.run(srv_tx).await }
        });
        sleep(Duration::from_millis(150)).await; // Wait for server to start

        // Connect client with Unix socket
        let cli_cfg = RapidClientConfig::new(socket_path.to_string(), true, Some(SocketType::Unix));
        let mut client = RapidClient::connect(&cli_cfg).await
            .expect("connect");

        // Verify server received connection
        let ev = tokio::time::timeout(Duration::from_secs(2), srv_rx.recv())
            .await
            .expect("srv ev timeout")
            .expect("no event");
        assert!(matches!(ev, ServerEvent::Connected{..}));

        // Send message from client to server
        client.send(make_msg("UnixKey", "UnixVal")).await.unwrap();

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
            b"UnixKey"
        );

        // Send response from server to client
        server
            .send_to_client(&cli_id, make_msg("", "UnixResp"))
            .await
            .unwrap();

        // Verify client received response
        sleep(Duration::from_millis(100)).await;
        let ans = client.recv().await.expect("no response");
        assert_eq!(
            ans.get_field(&FLD_VAL.into()).unwrap().value(),
            b"UnixResp"
        );
    }

    #[tokio::test]
    async fn test_unix_multiple_clients() {
        // Start server with Unix socket
        let socket_path = "/tmp/rapid_net_test_multi.sock";
        let cfg_srv = RapidServerConfig::new(socket_path.to_string(), true, Some(SocketType::Unix));
        let (tx_srv, mut rx_srv) = mpsc::channel::<ServerEvent>(200);
        let srv = Arc::new(RapidServer::new(cfg_srv));
        tokio::spawn({
            let s = srv.clone();
            async move { s.run(tx_srv).await }
        });
        sleep(Duration::from_millis(120)).await;

        // Connect multiple clients with Unix socket
        let mut clients = Vec::new();
        for _ in 0..3 {
            let cfg = RapidClientConfig::new(socket_path.to_string(), true, Some(SocketType::Unix));
            let cli = RapidClient::connect(&cfg).await.unwrap();
            clients.push(cli);
        }

        // Drain the Connected events
        while let Ok(Some(ServerEvent::Connected{..})) =
            tokio::time::timeout(Duration::from_millis(20), rx_srv.recv()).await {}

        // Each client sends a message
        for (i, cli) in clients.iter().enumerate() {
            cli.send(make_msg(&format!("unix_k{i}"), &format!("unix_v{i}")))
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
        assert_eq!(received.len(), clients.len(), "not all Unix messages seen");
    }

    #[tokio::test]
    async fn test_unix_vs_tcp_performance() {
        // This test compares the performance of Unix sockets vs TCP sockets
        // by measuring the time it takes to send and receive a large message

        // Create a large message
        let mut large_msg = RapidTlvMessage::new(EVT_MSG.into());
        let large_data = vec![0u8; 1024 * 1024]; // 1MB of data
        large_msg.add_field(FLD_VAL.into(), Bytes::from(large_data));

        // Test Unix socket performance
        let unix_socket_path = "/tmp/rapid_net_perf_unix.sock";
        let unix_start = std::time::Instant::now();
        
        // Start Unix server
        let unix_srv_cfg = RapidServerConfig::new(unix_socket_path.to_string(), true, Some(SocketType::Unix));
        let (unix_srv_tx, mut unix_srv_rx) = mpsc::channel::<ServerEvent>(100);
        let unix_server = Arc::new(RapidServer::new(unix_srv_cfg));
        let _unix_srv_handle = tokio::spawn({
            let s = unix_server.clone();
            async move { s.run(unix_srv_tx).await }
        });
        sleep(Duration::from_millis(150)).await;

        // Connect Unix client
        let unix_cli_cfg = RapidClientConfig::new(unix_socket_path.to_string(), true, Some(SocketType::Unix));
        let mut unix_client = RapidClient::connect(&unix_cli_cfg).await.expect("unix connect");

        // Wait for connection event
        let _ = tokio::time::timeout(Duration::from_secs(2), unix_srv_rx.recv()).await;

        // Send large message over Unix socket
        unix_client.send(large_msg.clone()).await.unwrap();

        // Wait for message to be received
        let _ = tokio::time::timeout(Duration::from_secs(5), unix_srv_rx.recv()).await;
        let unix_duration = unix_start.elapsed();

        // Test TCP socket performance
        let tcp_port = 19880;
        let tcp_addr = format!("127.0.0.1:{tcp_port}");
        let tcp_start = std::time::Instant::now();
        
        // Start TCP server
        let tcp_srv_cfg = RapidServerConfig::new(tcp_addr.clone(), true, Some(SocketType::Tcp));
        let (tcp_srv_tx, mut tcp_srv_rx) = mpsc::channel::<ServerEvent>(100);
        let tcp_server = Arc::new(RapidServer::new(tcp_srv_cfg));
        let _tcp_srv_handle = tokio::spawn({
            let s = tcp_server.clone();
            async move { s.run(tcp_srv_tx).await }
        });
        sleep(Duration::from_millis(150)).await;

        // Connect TCP client
        let tcp_cli_cfg = RapidClientConfig::new(tcp_addr.clone(), true, Some(SocketType::Tcp));
        let mut tcp_client = RapidClient::connect(&tcp_cli_cfg).await.expect("tcp connect");

        // Wait for connection event
        let _ = tokio::time::timeout(Duration::from_secs(2), tcp_srv_rx.recv()).await;

        // Send large message over TCP socket
        tcp_client.send(large_msg.clone()).await.unwrap();

        // Wait for message to be received
        let _ = tokio::time::timeout(Duration::from_secs(5), tcp_srv_rx.recv()).await;
        let tcp_duration = tcp_start.elapsed();

        println!("Unix socket performance: {:?}", unix_duration);
        println!("TCP socket performance: {:?}", tcp_duration);
        println!("Unix socket is {:.2}x faster than TCP", tcp_duration.as_secs_f64() / unix_duration.as_secs_f64());
    }
}

// This module is empty on non-Unix platforms
#[cfg(not(unix))]
mod unix_tests {
    #[test]
    fn dummy_test() {
        // This test is just a placeholder for non-Unix platforms
        println!("Unix socket tests are skipped on non-Unix platforms");
    }
}