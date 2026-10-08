//! Discord gateway, REST client, state store and SQLite disk cache for Akari.

mod account;
pub mod auth;
mod backlog;
mod backoff;
mod client;
mod error;
pub mod gateway;
mod heartbeat;
mod lenient;
pub mod model;
pub mod properties;
mod random;
mod rest;
mod secret;
pub mod state;
mod tls;
pub mod token_store;
mod ws;

pub use account::Account;
pub use client::{ClientError, DiscordClient, Endpoints};
pub use error::{JsonError, JsonErrorKind, TransportError, TransportErrorKind};
pub use secret::{Secret, Token};
pub use token_store::{TokenStore, TokenStoreError};
