//! The account's state as a UI reads it, kept current by the gateway.

// No callers outside tests until the store exists.
#[cfg_attr(not(test), allow(dead_code))]
mod apply;
mod convert;
mod events;
mod permissions;
mod types;
mod windows;

pub use events::{ConnectionState, StoreEvent};
pub use types::*;
pub use windows::MessageWindow;
