//! The account's state as a UI reads it, kept current by the gateway.

mod apply;
mod convert;
mod events;
mod permissions;
mod store;
mod types;
// Account's history loads will be the first callers outside tests.
#[cfg_attr(not(test), allow(dead_code))]
mod windows;

pub use events::{ConnectionState, StoreEvent};
pub use store::{Store, Subscription};
pub use types::*;
pub use windows::MessageWindow;
pub(crate) use windows::{DEFAULT_LIMITS, WindowLimits};
