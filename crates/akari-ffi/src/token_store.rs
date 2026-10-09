use std::sync::Arc;

use akari_core::model::UserId;

use crate::errors::TokenStoreError;

/// Persistent token storage, keyed by account; the host implements it with the platform's
/// secret store. Called only from background threads, where it may block or show a dialog.
#[uniffi::export(foreign)]
pub trait TokenStore: Send + Sync {
    fn load(&self, account: UserId) -> Result<Option<String>, TokenStoreError>;
    fn save(&self, account: UserId, token: String) -> Result<(), TokenStoreError>;
    /// Deleting a token that isn't stored succeeds.
    fn delete(&self, account: UserId) -> Result<(), TokenStoreError>;
}

/// The host's store as akari-core sees it.
pub(crate) struct HostStore(pub(crate) Arc<dyn TokenStore>);

impl akari_core::TokenStore for HostStore {
    fn load(
        &self,
        account: UserId,
    ) -> Result<Option<akari_core::Token>, akari_core::TokenStoreError> {
        Ok(self.0.load(account)?.map(akari_core::Token::new))
    }

    fn save(
        &self,
        account: UserId,
        token: &akari_core::Token,
    ) -> Result<(), akari_core::TokenStoreError> {
        Ok(self.0.save(account, token.expose().to_owned())?)
    }

    fn delete(&self, account: UserId) -> Result<(), akari_core::TokenStoreError> {
        Ok(self.0.delete(account)?)
    }
}
