//! UniFFI bindings over `akari-core` and `akari-markdown` for Swift and Kotlin.

mod client;
mod errors;
mod ids;
mod logging;
mod login;
mod runtime;
mod token_store;

#[cfg(test)]
mod tests;

pub use client::{DiscordClient, Endpoints, HostInfo, Token, discord_endpoints};
pub use errors::{ClientError, LoginError, LogoutError, NetworkErrorKind, TokenStoreError};
pub use logging::enable_logging;
pub use login::{
    CaptchaChallenge, LoginStep, LoginSuccess, MfaChallenge, MfaMethod, NewLocation, PasswordLogin,
    QrEvent, QrLogin, ScannedUser,
};
pub use token_store::TokenStore;

uniffi::setup_scaffolding!();
