//! Discord gateway, REST client, state store and SQLite disk cache for Akari.

pub mod auth;
mod client;
mod error;
pub mod gateway;
pub mod model;
pub mod properties;
mod rest;
mod secret;
mod tls;
pub mod token_store;

pub use client::{ClientError, DiscordClient, Endpoints};
pub use error::{TransportError, TransportErrorKind};
pub use secret::{Secret, Token};
pub use token_store::{TokenStore, TokenStoreError};
