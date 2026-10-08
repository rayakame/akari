//! Where tokens live between launches. The host implements it with the platform's secret
//! store (Keychain, Android Keystore, libsecret).

use crate::Token;
use crate::model::{Snowflake, UserMarker};

/// Persistent token storage, keyed by account.
///
/// Calls may block or show a system dialog, so akari-core only makes them from a blocking
/// thread, never on the async runtime.
pub trait TokenStore: Send + Sync + 'static {
    fn load(&self, account: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError>;
    fn save(&self, account: Snowflake<UserMarker>, token: &Token) -> Result<(), TokenStoreError>;
    /// Deleting a token that isn't stored succeeds.
    fn delete(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    /// The platform store reported an error. The message must not contain the token.
    #[error("token storage failed: {0}")]
    Backend(String),
    /// The store can't be used right now, for example a locked keychain.
    #[error("token storage is unavailable")]
    Unavailable,
}
