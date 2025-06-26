pub mod io_core;
pub mod inbound;
pub mod outbound;
mod error;
/* internes Low-Level-Kernstück ─ nur crate-weit sichtbar */
pub(crate) use io_core::IoCore;

/* öffentliche Typen, die andere nutzen sollen */
pub use inbound::InboundClient;
pub use outbound::OutboundClient;