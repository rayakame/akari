//! Gateway payloads as Discord sends them.

mod hello;
mod payload;

pub use hello::Hello;
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
