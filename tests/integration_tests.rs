use std::{collections::HashSet, sync::Arc, time::Duration};

use bytes::Bytes;
use rapid_net::{
    config::{RapidClientConfig, RapidServerConfig},
    message::ServerEvent,
    RapidClient, RapidServer,
};
use rapid_tlv::RapidTlvMessage;

use tokio::{sync::mpsc, time::sleep};
use uuid::Uuid;

/* eigene Field- und Event-Konstanten ------------------------------------- */
const EVT_MSG: u8 = 0x10;
const FLD_KEY: u8 = 0x10;
const FLD_VAL: u8 = 0x11;

/* Hilfsfunktion: TLV-Nachricht bauen ------------------------------------- */
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
/* 1. Ein Client ↔ Server                                                   */
/* ------------------------------------------------------------------------ */
#[tokio::test]
async fn test_server_client_communication() {
    // ───── Server starten ────────────────────────────────────────────────
    let port        = 19_876;
    let addr_str    = format!("127.0.0.1:{port}");
    let srv_cfg     = RapidServerConfig::new(addr_str.clone(), true);
    let (srv_tx, mut srv_rx) = mpsc::channel::<ServerEvent>(100);
    let server      = Arc::new(RapidServer::new(srv_cfg));
    let _srv_handle = tokio::spawn({
        let s = server.clone();
        async move { s.run(srv_tx).await }
    });
    sleep(Duration::from_millis(150)).await; // kurz warten

    // ───── Client verbindet sich ────────────────────────────────────────
    let cli_cfg  = RapidClientConfig::new(addr_str.clone(), true);
    let mut client = RapidClient::connect(&cli_cfg).await
        .expect("connect");

    // Server-Connected bestätigen
    let ev = tokio::time::timeout(Duration::from_secs(2), srv_rx.recv())
        .await
        .expect("srv ev timeout")
        .expect("no event");
    assert!(matches!(ev, ServerEvent::Connected{..}));

    // ───── Client → Server Nachricht ─────────────────────────────────────
    client.send(make_msg("TestKey", "TestVal")).await.unwrap();

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
        b"TestKey"
    );

    // ───── Server → Client Antwort ───────────────────────────────────────
    server
        .send_to_client(&cli_id, make_msg("", "SrvResp"))
        .await
        .unwrap();

    sleep(Duration::from_millis(100)).await;          // ankommen lassen
    let ans = client.recv().await.expect("no response");
    assert_eq!(
        ans.get_field(&FLD_VAL.into()).unwrap().value(),
        b"SrvResp"
    );

}

/* ------------------------------------------------------------------------ */
/* 2. Drei gleichzeitige Clients                                            */
/* ------------------------------------------------------------------------ */
#[tokio::test]
async fn test_multiple_clients() {
    let port     = 19_877;
    let addr_str = format!("127.0.0.1:{port}");
    let cfg_srv  = RapidServerConfig::new(addr_str.clone(), true);
    let (tx_srv, mut rx_srv) = mpsc::channel::<ServerEvent>(200);
    let srv      = Arc::new(RapidServer::new(cfg_srv));
    tokio::spawn({
        let s = srv.clone();
        async move { s.run(tx_srv).await }
    });
    sleep(Duration::from_millis(120)).await;

    // ───── drei Clients verbinden ───────────────────────────────────────
    let mut clients = Vec::new();
    for _ in 0..3 {
        let cfg = RapidClientConfig::new(addr_str.clone(), true);
        let cli = RapidClient::connect(&cfg).await.unwrap();
        clients.push(cli);
    }
    // Drain der Connected-Events
    while let Ok(Some(ServerEvent::Connected{..})) =
        tokio::time::timeout(Duration::from_millis(20), rx_srv.recv()).await {}

    // ───── jeder Client schickt eine Nachricht ───────────────────────────
    for (i, cli) in clients.iter().enumerate() {
        cli.send(make_msg(&format!("k{i}"), &format!("v{i}")))
            .await
            .unwrap();
    }

    // ───── Server empfängt alle ──────────────────────────────────────────
    let mut received = HashSet::<Uuid>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while received.len() < clients.len() && tokio::time::Instant::now() < deadline {
        if let Ok(Some(ServerEvent::Message{client_id, ..})) =
            tokio::time::timeout(Duration::from_millis(200), rx_srv.recv()).await
        {
            received.insert(client_id);
        }
    }
    assert_eq!(received.len(), clients.len(), "not all messages seen");
}
