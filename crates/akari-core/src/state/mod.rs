//! The account's state as a UI reads it, kept current by the gateway.

// Removed once the store converts dispatches (Task 5).
#[cfg_attr(not(test), allow(dead_code))]
mod convert;
mod types;

pub use types::*;
