use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpListener;

use rapid_net::{
    config::RapidClientConfig
    ,
    RapidClient,
};

use rapid_tlv::RapidTlvMessage;

/// Hilfs-Funktion: startet einen Dummy-Server, liefert Addr + JoinHandle
async fn spawn_dummy_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        // Akzeptiere exakt **eine** Verbindung und ignoriere alles
        let _ = listener.accept().await;
    });
    (addr.to_string(), handle)
}

/* -------------------------------------------------------------------------- */
/* 1. Encoding / Decoding                                                     */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn test_message_encode_decode() {
    let mut msg = RapidTlvMessage::new(0x01.into());           // Dummy-Event-Type
    msg.add_field(
        0x10,
        bytes::Bytes::from_static(b"unit_key"),
    );
    msg.add_field(
        0x11,
        bytes::Bytes::from_static(b"unit_value"),
    );

    let encoded = msg.encode().expect("encode");
    let parsed  = RapidTlvMessage::parse(bytes::Bytes::copy_from_slice(&encoded))
        .expect("parse");

    assert_eq!(parsed.event_type, msg.event_type);
    assert_eq!(
        parsed.get_field(&0x10).unwrap().value(),
        b"unit_key"
    );
    assert_eq!(
        parsed.get_field(&0x11).unwrap().value(),
        b"unit_value"
    );
}

/* -------------------------------------------------------------------------- */
/* 2. Fehler-Handling: ungültiges Paket                                       */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn test_message_parse_error() {
    // len = 5 (4 header-bytes + 1 event-byte)
    // event-type = 0xFF  → nicht definiert
    let bogus = bytes::Bytes::from_static(&[0, 0, 0, 5]);
    assert!(RapidTlvMessage::parse(bogus).is_err());
}

/* -------------------------------------------------------------------------- */
/* 3. Reconnect-Logik                                                         */
/* -------------------------------------------------------------------------- */
async fn spawn_server_on(addr: String) -> tokio::task::JoinHandle<()> {
    let listener = TcpListener::bind(&addr).await.unwrap();
    tokio::spawn(async move { let _ = listener.accept().await; })
}

#[tokio::test]
async fn test_client_reconnect() {
    // 1) freien Port finden und wieder freigeben
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_str = listener.local_addr().unwrap().to_string();
    drop(listener);

    let cfg = RapidClientConfig::new(addr_str.clone(), true);

    // erster Versuch muss scheitern
    assert!(RapidClient::connect(&cfg).await.is_err());

    // 2) Server GENAU auf diesen Port starten
    let _srv = spawn_server_on(addr_str.clone()).await;

    // zweiter Versuch klappt
    let client = RapidClient::connect(&cfg).await.expect("reconnect ok");
    assert!(client.is_alive());
}

/* -------------------------------------------------------------------------- */
/* 4. Connection-Timeout                                                      */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn test_client_connect_timeout() {
    // OFFSET-Adresse aus TEST-NET-1 – sollte sofort 'unreachable' liefern
    let cfg = RapidClientConfig::new("192.0.2.1:65000".into(), true);

    let res = tokio::time::timeout(Duration::from_secs(3), RapidClient::connect(&cfg)).await;
    assert!(res.is_err() || res.unwrap().is_err(), "erwarteter Timeout/Refused");
}
