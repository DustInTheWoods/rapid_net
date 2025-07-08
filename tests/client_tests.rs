use std::time::Duration;
use tokio::net::TcpListener;

use rapid_net::{config::RapidClientConfig, RapidClient};
use rapid_tlv::RapidTlvMessage;

/// Dummy-Server: akzeptiert genau **eine** TCP-Verbindung und beendet sich dann.
async fn spawn_server_on(addr: &str) -> tokio::task::JoinHandle<()> {
    // bind schlägt fehl → Test panic, gut so
    let listener = TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        // ignorieren, ob accept() klappt – der Test erledigt das Abbrechen
        let _ = listener.accept().await;
    })
}

/* -------------------------------------------------------------------------- */
/* 1. Encoding / Decoding                                                     */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn message_encode_decode() {
    let mut msg = RapidTlvMessage::new(0x01); // Dummy-Event-Type
    msg.add_field(0x10, bytes::Bytes::from_static(b"unit_key"));
    msg.add_field(0x11, bytes::Bytes::from_static(b"unit_value"));

    let encoded = msg.encode().expect("encode");
    let parsed = RapidTlvMessage::parse(bytes::Bytes::copy_from_slice(&encoded))
        .expect("parse");

    assert_eq!(parsed.event_type, msg.event_type);
    assert_eq!(parsed.get_field(&0x10).unwrap().value(), b"unit_key");
    assert_eq!(parsed.get_field(&0x11).unwrap().value(), b"unit_value");
}

/* -------------------------------------------------------------------------- */
/* 2. Fehler-Handling: ungültiges Paket                                       */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn message_parse_error() {
    // Nur Header (4 Bytes) → Länge = 4, keine Payload → sollte Error liefern
    let bogus = bytes::Bytes::from_static(&[0, 0, 0, 4]);
    assert!(RapidTlvMessage::parse(bogus).is_err());
}

/* -------------------------------------------------------------------------- */
/* 3. Reconnect-Logik                                                         */
/* -------------------------------------------------------------------------- */
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_reconnect() {
    // 1) freien Port sichern und direkt wieder freigeben
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);

    let cfg = RapidClientConfig::new(addr.clone(), true);

    // erster Versuch MUSS scheitern
    assert!(RapidClient::connect(&cfg).await.is_err());

    // 2) Server exakt auf diesem Port starten
    let srv = spawn_server_on(&addr).await;

    // zweiter Versuch MUSS klappen
    let client = RapidClient::connect(&cfg)
        .await
        .expect("reconnect ok");
    assert!(client.is_alive());

    // Dummy-Server beenden, sonst hängt der Test-Runner
    srv.abort();
}

/* -------------------------------------------------------------------------- */
/* 4. Connection-Timeout                                                      */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn client_connect_timeout() {
    // 192.0.2.0/24 = TEST-NET-1 → sollte sofort ECONNREFUSED oder Timeout liefern
    let cfg = RapidClientConfig::new("192.0.2.1:65000".into(), true);

    let res = tokio::time::timeout(Duration::from_secs(3), RapidClient::connect(&cfg)).await;
    // Timeout der Future ODER Result::Err beiderseits sind erlaubt
    assert!(
        res.is_err() || res.unwrap().is_err(),
        "erwarteter Timeout oder Connection Refused"
    );
}
