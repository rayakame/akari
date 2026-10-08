//! The account's state as a UI reads it, kept current by the gateway.

// Nothing outside tests creates a store until Account does.
#[cfg_attr(not(test), allow(dead_code))]
mod apply;
#[cfg_attr(not(test), allow(dead_code))]
mod convert;
mod events;
#[cfg_attr(not(test), allow(dead_code))]
mod permissions;
#[cfg_attr(not(test), allow(dead_code))]
mod store;
mod types;
#[cfg_attr(not(test), allow(dead_code))]
mod windows;

pub use events::{ConnectionState, StoreEvent};
pub use store::{Store, Subscription};
pub use types::*;
pub use windows::MessageWindow;
