//! The account's state as a UI reads it, kept current by the gateway.

// No callers outside tests until the store and the message windows exist.
#[cfg_attr(not(test), allow(dead_code))]
mod apply;
#[cfg_attr(not(test), allow(dead_code))]
mod convert;
mod events;
mod permissions;
mod types;

pub use events::{ConnectionState, StoreEvent};
pub use types::*;
