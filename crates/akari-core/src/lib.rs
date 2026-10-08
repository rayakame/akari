//! Discord gateway, REST client, state store and SQLite disk cache for Akari.

pub mod auth;
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
mod tls;
pub mod token_store;
mod ws;

pub use client::{ClientError, DiscordClient, Endpoints};
pub use error::{TransportError, TransportErrorKind};
pub use secret::{Secret, Token};
pub use token_store::{TokenStore, TokenStoreError};
