//! Gateway payloads as Discord sends them, and the connection that receives them.

mod connection;
mod decompress;
mod guild;
mod hello;
mod lenient;
mod limiter;
mod outgoing;
mod payload;
mod ready;
pub(crate) mod session;

pub use connection::{ConnectionEvent, DisconnectReason, Gateway, GatewayError, SendError};
pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use outgoing::{GatewayCommand, PresenceStatus};
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use ready::Ready;
