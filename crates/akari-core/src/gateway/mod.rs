//! Gateway payloads as Discord sends them, and the connection that receives them.

mod connection;
mod decompress;
mod guild;
mod hello;
mod lenient;
#[allow(dead_code)]
mod limiter;
#[allow(dead_code)]
mod outgoing;
mod payload;
mod ready;
#[allow(dead_code)]
pub(crate) mod session;

pub use connection::{ConnectionEvent, DisconnectReason, Gateway, GatewayError, SendError};
pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use outgoing::{GatewayCommand, PresenceStatus};
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use ready::Ready;
