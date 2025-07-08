//! src/client/error.rs

use std::io;
use uuid::Uuid;
use thiserror::Error;
use rapid_tlv::RapidTlvError;

/// Sammel-Fehlertyp für alles, was auf Client-Seite schiefgehen kann.
#[derive(Debug, Error)]
pub enum ClientError {
    /* ───────────── Transport / Socket ───────────── */
    #[error("Verbindung unerwartet geschlossen")]
    ConnectionClosed,

    #[error("I/O-Fehler: {0}")]
    Io(#[from] io::Error),

    #[error("Timeout beim Lesen des Sockets")]
    ReadTimeout,

    #[error("Timeout beim Schreiben des Sockets")]
    WriteTimeout,

    #[error("Timeout beim Flush")]
    FlushTimeout,

    /* ───────────── Protokoll / Codec ───────────── */
    #[error("Rapid-TLV-Fehler: {0:?}")]
    Codec(RapidTlvError),

    #[error("Nachricht zu groß ({0} B)")]
    MessageTooLarge(usize),

    /* ───────────── Channel / Task ───────────── */
    #[error("MPSC-Kanal zu Schreib-Task ist geschlossen")]
    ChannelClosed,

    #[error("Timeout beim Senden in Schreib-Kanal")]
    ChannelSendTimeout,

    /* ───────────── Anwendungsebene ───────────── */
    #[error("Peer {0} meldet Fehler: {1}")]
    RemoteError(Uuid, String),
}

/* RapidTlvError ↔ ClientError */
impl From<RapidTlvError> for ClientError {
    fn from(e: RapidTlvError) -> Self {
        ClientError::Codec(e)
    }
}
