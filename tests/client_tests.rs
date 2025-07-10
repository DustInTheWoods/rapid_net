use std::time::Duration;
use tokio::net::TcpListener;

use rapid_net::{config::RapidClientConfig, RapidClient};
use rapid_tlv::RapidTlvMessage;

/// Dummy server: accepts exactly **one** TCP connection and then terminates.
async fn spawn_server_on(addr: &str) -> tokio::task::JoinHandle<()> {
    // bind failure -> test panic, which is expected
    let listener = TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        // ignore whether accept() succeeds - the test handles the termination
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
/* 2. Error Handling: Invalid Packet                                          */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn message_parse_error() {
    // Only header (4 bytes) -> Length = 4, no payload -> should return an error
    let bogus = bytes::Bytes::from_static(&[0, 0, 0, 4]);
    assert!(RapidTlvMessage::parse(bogus).is_err());
}

/* -------------------------------------------------------------------------- */
/* 3. Reconnect Logic                                                         */
/* -------------------------------------------------------------------------- */
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_reconnect() {
    // 1) secure a free port and release it immediately
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);

    let cfg = RapidClientConfig::new(addr.clone(), true);

    // first attempt MUST fail
    assert!(RapidClient::connect(&cfg).await.is_err());

    // 2) start server on exactly this port
    let srv = spawn_server_on(&addr).await;

    // second attempt MUST succeed
    let client = RapidClient::connect(&cfg)
        .await
        .expect("reconnect ok");
    assert!(client.is_alive());

    // terminate dummy server, otherwise the test runner will hang
    srv.abort();
}

/* -------------------------------------------------------------------------- */
/* 4. Connection Timeout                                                      */
/* -------------------------------------------------------------------------- */
#[tokio::test]
async fn client_connect_timeout() {
    // 192.0.2.0/24 = TEST-NET-1 -> should immediately return ECONNREFUSED or Timeout
    let cfg = RapidClientConfig::new("192.0.2.1:65000".into(), true);

    let res = tokio::time::timeout(Duration::from_secs(3), RapidClient::connect(&cfg)).await;
    // Both Future timeout OR Result::Err are acceptable
    assert!(
        res.is_err() || res.unwrap().is_err(),
        "expected Timeout or Connection Refused"
    );
}
