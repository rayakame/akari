//! Gateway payloads as Discord sends them.

#[cfg_attr(not(test), allow(dead_code))]
mod decompress;
mod guild;
mod hello;
mod lenient;
mod payload;
mod ready;

pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use ready::Ready;
