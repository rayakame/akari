use std::time::Duration;

use akari_core::TransportErrorKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
#[uniffi::export(Display)]
pub enum ClientError {
    #[error("couldn't set up TLS")]
    Tls,
    #[error("couldn't set up the HTTP client")]
    Http,
    #[error("invalid {name} endpoint")]
    InvalidEndpoint { name: String },
    #[error("a client property can't be sent as the {header} header")]
    InvalidProperties { header: String },
    #[error("Discord has no desktop client for this platform")]
    UnsupportedPlatform,
    #[error("couldn't start Akari's background runtime")]
    Runtime,
    #[error("invalid log filter")]
    InvalidLogFilter,
}

impl From<akari_core::ClientError> for ClientError {
    fn from(err: akari_core::ClientError) -> Self {
        use akari_core::ClientError as E;

        tracing::error!(error = %err, "couldn't set up the client");
        match err {
            E::Tls(_) => Self::Tls,
            E::Http(_) => Self::Http,
            E::InvalidEndpoint(name) => Self::InvalidEndpoint {
                name: name.to_owned(),
            },
            E::InvalidProperties(header) => Self::InvalidProperties {
                header: header.to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
#[uniffi::export(Display)]
pub enum TokenStoreError {
    /// The platform store reported an error. The message must not contain the token.
    #[error("token storage failed: {message}")]
    Backend { message: String },
    /// The store can't be used right now, for example a locked keychain.
    #[error("token storage is unavailable")]
    Unavailable,
}

// A host error of another type; without this conversion UniFFI would panic.
impl From<uniffi::UnexpectedUniFFICallbackError> for TokenStoreError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Backend {
            message: err.reason,
        }
    }
}

impl From<TokenStoreError> for akari_core::TokenStoreError {
    fn from(err: TokenStoreError) -> Self {
        match err {
            TokenStoreError::Backend { message } => Self::Backend(message),
            TokenStoreError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<akari_core::TokenStoreError> for TokenStoreError {
    fn from(err: akari_core::TokenStoreError) -> Self {
        match err {
            akari_core::TokenStoreError::Backend(message) => Self::Backend { message },
            akari_core::TokenStoreError::Unavailable => Self::Unavailable,
        }
    }
}

/// What kind of network failure happened, without the underlying error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NetworkErrorKind {
    Connect,
    Timeout,
    Tls,
    /// The other side broke the HTTP or WebSocket protocol.
    Protocol,
    Other,
}

impl From<TransportErrorKind> for NetworkErrorKind {
    fn from(kind: TransportErrorKind) -> Self {
        match kind {
            TransportErrorKind::Connect => Self::Connect,
            TransportErrorKind::Timeout => Self::Timeout,
            TransportErrorKind::Tls => Self::Tls,
            TransportErrorKind::Protocol => Self::Protocol,
            TransportErrorKind::Other => Self::Other,
        }
    }
}

/// Why logging out failed. The stored token is deleted even when Discord can't be reached,
/// so only `Storage` means it is still there.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
#[uniffi::export(Display)]
pub enum LogoutError {
    #[error("not logged in")]
    NotLoggedIn,
    #[error("couldn't remove the stored token")]
    Storage,
    #[error("network error")]
    Network { kind: NetworkErrorKind },
    #[error("rate limited by Discord")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Discord error {code}: {message}")]
    Discord { code: u32, message: String },
    #[error("unexpected response from Discord")]
    UnexpectedResponse,
}

impl From<akari_core::auth::LogoutError> for LogoutError {
    fn from(err: akari_core::auth::LogoutError) -> Self {
        use akari_core::auth::LogoutError as E;

        match err {
            E::NotLoggedIn => Self::NotLoggedIn,
            E::Storage(err) => {
                tracing::warn!(error = %err, "couldn't remove the stored token");
                Self::Storage
            }
            E::Network(err) => Self::Network {
                kind: err.kind().into(),
            },
            E::RateLimited { retry_after } => Self::RateLimited { retry_after },
            E::Discord { code, message } => Self::Discord { code, message },
            E::UnexpectedResponse => Self::UnexpectedResponse,
        }
    }
}
