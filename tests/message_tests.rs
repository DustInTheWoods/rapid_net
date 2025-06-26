use rapid_net::message::ClientEvent;
use rapid_tlv::{RapidTlvMessage, RapidTlvEventType, RapidTlvFieldType};
use bytes::Bytes;
use uuid::Uuid;
use std::net::SocketAddr;

pub const MSG_EVENT: u8 = 0x01;
pub const MSG_CONNECTED: u8 = 0x02;
pub const MSG_DISCONNECTED: u8 = 0x03;
pub const MSG_ERROR: u8 = 0x04;

pub const FIELD_KEY: u8 = 0x01;
pub const FIELD_VALUE: u8 = 0x02;

// We'll need to mock RapidClient for ServerEvent tests
struct MockClient {
    id: Uuid,
    addr: SocketAddr,
}

impl MockClient {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn addr(&self) -> SocketAddr {
        self.addr
    }
}

#[test]
fn test_client_event_message() {
    let client_id = Uuid::new_v4();

    // Create a message for the event
    let mut message = RapidTlvMessage::new(MSG_EVENT);
    message.add_field(FIELD_KEY, Bytes::from("TestKey"));

    // Create the event with the message
    let event = ClientEvent::Message {
        client_id,
        message,
    };

    // We can't easily match on ClientEvent variants directly since they don't implement PartialEq
    // Instead, we'll use pattern matching to verify the event
    match event {
        ClientEvent::Message { client_id: id, message: msg } => {
            assert_eq!(id, client_id);

            // Verify message content
            let key_field = msg.get_field(&FIELD_KEY);
            assert!(key_field.is_some());
            assert_eq!(
                String::from_utf8_lossy(key_field.unwrap().value()),
                "TestKey"
            );
        },
        _ => panic!("Expected ClientEvent::Message"),
    }
}

#[test]
fn test_client_event_connected() {
    let client_id = Uuid::new_v4();

    let event = ClientEvent::Connected {
        client_id,
    };

    match event {
        ClientEvent::Connected { client_id: id } => {
            assert_eq!(id, client_id);
        },
        _ => panic!("Expected ClientEvent::Connected"),
    }
}

#[test]
fn test_client_event_disconnected() {
    let client_id = Uuid::new_v4();

    let event = ClientEvent::Disconnected {
        client_id,
    };

    match event {
        ClientEvent::Disconnected { client_id: id } => {
            assert_eq!(id, client_id);
        },
        _ => panic!("Expected ClientEvent::Disconnected"),
    }
}

#[test]
fn test_client_event_error() {
    let client_id = Uuid::new_v4();
    let error_message = "Test error message";

    let event = ClientEvent::Error {
        client_id,
        error: error_message.to_string(),
    };

    match event {
        ClientEvent::Error { client_id: id, error } => {
            assert_eq!(id, client_id);
            assert_eq!(error, error_message);
        },
        _ => panic!("Expected ClientEvent::Error"),
    }
}

#[test]
fn test_message_creation_and_fields() {
    // Create a new message
    let mut message = RapidTlvMessage::new(MSG_EVENT);

    // Add fields
    message.add_field(FIELD_KEY, Bytes::from("TestKey"));
    message.add_field(FIELD_VALUE, Bytes::from("TestValue"));

    // Verify fields
    let key_field = message.get_field(&FIELD_KEY);
    assert!(key_field.is_some());
    assert_eq!(
        String::from_utf8_lossy(key_field.unwrap().value()),
        "TestKey"
    );

    let value_field = message.get_field(&FIELD_VALUE);
    assert!(value_field.is_some());
    assert_eq!(
        String::from_utf8_lossy(value_field.unwrap().value()),
        "TestValue"
    );

    // Test field removal
    message.remove_field(FIELD_KEY);
    assert!(message.get_field(&FIELD_KEY).is_none());
    assert!(message.get_field(&FIELD_VALUE).is_some());
}

#[test]
fn test_message_encoding_decoding() {
    // Create a message with fields
    let mut original = RapidTlvMessage::new(MSG_EVENT);
    original.add_field(FIELD_KEY, Bytes::from("TestKey"));
    original.add_field(FIELD_VALUE, Bytes::from("TestValue"));

    // Encode the message
    let encoded = original.encode().expect("Failed to encode message");

    // Parse the encoded message
    let parsed = RapidTlvMessage::parse(Bytes::from(encoded.to_vec())).expect("Failed to parse message");

    // Verify the parsed message has the same fields
    let key_field = parsed.get_field(&FIELD_KEY);
    assert!(key_field.is_some());
    assert_eq!(
        String::from_utf8_lossy(key_field.unwrap().value()),
        "TestKey"
    );

    let value_field = parsed.get_field(&FIELD_VALUE);
    assert!(value_field.is_some());
    assert_eq!(
        String::from_utf8_lossy(value_field.unwrap().value()),
        "TestValue"
    );
}
