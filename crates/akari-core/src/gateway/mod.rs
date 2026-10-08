//! Gateway payloads as Discord sends them.

#[cfg_attr(not(test), allow(dead_code))]
mod decompress;
mod guild;
mod hello;
mod lenient;
#[cfg_attr(not(test), allow(dead_code))]
mod limiter;
#[cfg_attr(not(test), allow(dead_code))]
mod outgoing;
mod payload;
mod ready;
#[allow(dead_code)]
mod session;

pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use outgoing::{GatewayCommand, PresenceStatus};
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use ready::Ready;
