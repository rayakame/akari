//! UniFFI bindings over `akari-core` and `akari-markdown` for Swift and Kotlin.

mod account;
mod client;
mod errors;
mod ids;
mod logging;
mod login;
mod records;
mod runtime;
mod store;
mod subscription;
mod token_store;

#[cfg(test)]
mod tests;

pub use account::Account;
pub use client::{DiscordClient, Endpoints, HostInfo, Token, discord_endpoints};
pub use errors::{
    ClientError, GatewayError, LoginError, LogoutError, NetworkErrorKind, TokenStoreError,
};
pub use logging::enable_logging;
pub use login::{
    CaptchaChallenge, LoginStep, LoginSuccess, MfaChallenge, MfaMethod, NewLocation, PasswordLogin,
    QrEvent, QrLogin, ScannedUser,
};
pub use records::{
    Attachment, Channel, ConnectionState, Delivery, Guild, Message, MessageWindow, User,
};
pub use store::Store;
pub use subscription::{StoreEvent, StoreSubscription};
pub use token_store::TokenStore;

uniffi::setup_scaffolding!();
