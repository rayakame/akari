use akari_core::model::{Snowflake, UserMarker};
use akari_core::{Token, TokenStore, TokenStoreError};
use keyring::Entry;

const SERVICE: &str = "akari-cli";
const CURRENT_ACCOUNT: &str = "current-account";

pub struct KeychainStore;

// The token store plus the one account akari-cli treats as logged in.
pub trait Accounts: TokenStore {
    fn current_account(&self) -> Result<Option<Snowflake<UserMarker>>, TokenStoreError>;
    fn set_current_account(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError>;
}

impl TokenStore for KeychainStore {
    fn load(&self, account: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
        read(&account.get().to_string()).map(|token| token.map(Token::new))
    }

    fn save(&self, account: Snowflake<UserMarker>, token: &Token) -> Result<(), TokenStoreError> {
        entry(&account.get().to_string())?
            .set_password(token.expose())
            .map_err(storage_error)
    }

    fn delete(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
        remove(&account.get().to_string())
    }
}

impl Accounts for KeychainStore {
    fn current_account(&self) -> Result<Option<Snowflake<UserMarker>>, TokenStoreError> {
        parse_account(read(CURRENT_ACCOUNT)?)
    }

    fn set_current_account(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
        entry(CURRENT_ACCOUNT)?
            .set_password(&account.get().to_string())
            .map_err(storage_error)
    }
}

impl KeychainStore {
    pub fn clear_current_account(&self) -> Result<(), TokenStoreError> {
        remove(CURRENT_ACCOUNT)
    }
}

// A present but unreadable entry is an error, not "logged out": the token it points to
// would otherwise become unreachable.
fn parse_account(value: Option<String>) -> Result<Option<Snowflake<UserMarker>>, TokenStoreError> {
    value
        .map(|id| {
            id.parse().map(Snowflake::new).map_err(|_| {
                TokenStoreError::Backend("the current-account entry is unreadable".to_owned())
            })
        })
        .transpose()
}

fn entry(user: &str) -> Result<Entry, TokenStoreError> {
    Entry::new(SERVICE, user).map_err(storage_error)
}

fn read(user: &str) -> Result<Option<String>, TokenStoreError> {
    match entry(user)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(storage_error(err)),
    }
}

fn remove(user: &str) -> Result<(), TokenStoreError> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(storage_error(err)),
    }
}

fn storage_error(err: keyring::Error) -> TokenStoreError {
    match err {
        keyring::Error::NoStorageAccess(_) => TokenStoreError::Unavailable,
        // These carry the stored bytes, which may be a token.
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            TokenStoreError::Backend("the stored entry is unreadable".to_owned())
        }
        err => TokenStoreError::Backend(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use akari_core::TokenStoreError;

    use super::*;

    #[test]
    fn unreadable_entries_never_quote_their_bytes() {
        let err = storage_error(keyring::Error::BadEncoding(b"secret-token".to_vec()));

        assert!(!err.to_string().contains("secret-token"));
        assert!(!format!("{err:?}").contains("secret-token"));
    }

    #[test]
    fn current_account_entries_parse_or_fail_loudly() {
        assert_eq!(parse_account(None).unwrap(), None);
        assert_eq!(
            parse_account(Some("100000000000000001".to_owned())).unwrap(),
            Some(Snowflake::new(100_000_000_000_000_001))
        );
        assert!(matches!(
            parse_account(Some("not-an-id".to_owned())),
            Err(TokenStoreError::Backend(_))
        ));
    }

    #[test]
    fn locked_stores_are_unavailable() {
        let err = storage_error(keyring::Error::NoStorageAccess("locked".into()));

        assert!(matches!(err, TokenStoreError::Unavailable));
    }
}
