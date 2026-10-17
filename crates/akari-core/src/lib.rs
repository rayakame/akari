//! Discord gateway, REST client, state store and SQLite disk cache for Akari.

mod account;
pub mod auth;
mod backlog;
mod backoff;
mod cache;
mod client;
mod error;
pub mod gateway;
mod heartbeat;
mod lenient;
mod load_report;
pub mod model;
mod prepare;
pub mod properties;
mod random;
mod rest;
mod secret;
pub mod state;
mod tls;
pub mod token_store;
mod ws;

// Launch logs carry durations in whole milliseconds.
pub(crate) fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

// Tenths of a millisecond, for steps that often take less than one.
pub(crate) fn ms(duration: std::time::Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

pub use account::{Account, MessageLoad};
pub use client::{ClientError, DiscordClient, Endpoints};
pub use error::{JsonError, JsonErrorKind, TransportError, TransportErrorKind};
pub use load_report::{LoadReport, PageOutcome};
pub use prepare::MessagePreparer;
pub use rest::RequestError;
pub use secret::{Secret, Token};
pub use state::message_length;
pub use token_store::{TokenStore, TokenStoreError};
