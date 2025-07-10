//! src/client/error.rs

use std::io;
use uuid::Uuid;
use thiserror::Error;
use rapid_tlv::RapidTlvError;

/// Collective error type for everything that can go wrong on the client side.
#[derive(Debug, Error)]
pub enum ClientError {
    /* ───────────── Transport / Socket ───────────── */
    #[error("Connection unexpectedly closed")]
    ConnectionClosed,

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Timeout reading from socket")]
    ReadTimeout,

    #[error("Timeout writing to socket")]
    WriteTimeout,

    #[error("Timeout during flush")]
    FlushTimeout,

    /* ───────────── Protocol / Codec ───────────── */
    #[error("Rapid-TLV error: {0:?}")]
    Codec(RapidTlvError),

    #[error("Message too large ({0} bytes)")]
    MessageTooLarge(usize),

    /* ───────────── Channel / Task ───────────── */
    #[error("MPSC channel to write task is closed")]
    ChannelClosed,

    #[error("Timeout sending to write channel")]
    ChannelSendTimeout,

    /* ───────────── Application Level ───────────── */
    #[error("Peer {0} reports error: {1}")]
    RemoteError(Uuid, String),
}

/* RapidTlvError ↔ ClientError */
impl From<RapidTlvError> for ClientError {
    fn from(e: RapidTlvError) -> Self {
        ClientError::Codec(e)
    }
}
