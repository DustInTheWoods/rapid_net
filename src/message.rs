use rapid_tlv::RapidTlvMessage;
use uuid::Uuid;

pub enum ClientEvent {
    Message {
        client_id: Uuid,
        message: RapidTlvMessage,
    },

    Connected {
        client_id: Uuid,
    },

    Disconnected {
        client_id: Uuid,
    },

    Error {
        client_id: Uuid,
        error: String,
    },
}

#[derive(Debug)]
pub enum ServerEvent {
    Message {
        client_id: Uuid,
        message: RapidTlvMessage,
    },

    Connected {
        client_id: Uuid,
    },

    Disconnected {
        client_id: Uuid,
    },

    Error {
        client_id: Uuid,
        error: String,
    },
}