//! Gateway payloads as Discord sends them.

mod guild;
mod hello;
mod payload;
mod ready;

pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use ready::Ready;
