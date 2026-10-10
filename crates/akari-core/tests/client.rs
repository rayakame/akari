use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use akari_core::auth::LogoutError;
use akari_core::model::{Snowflake, UserMarker};
use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
use akari_core::{DiscordClient, Endpoints, Token, TokenStore, TokenStoreError};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT: Snowflake<UserMarker> = Snowflake::new(100_000_000_000_000_001);

#[derive(Default)]
struct MemoryStore {
    tokens: Mutex<HashMap<u64, String>>,
}

impl MemoryStore {
    fn holds(&self, account: Snowflake<UserMarker>) -> bool {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&account.get())
    }
}

impl TokenStore for MemoryStore {
    fn load(&self, account: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
        Ok(self
            .tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&account.get())
            .map(|token| Token::new(token.clone())))
    }

    fn save(&self, account: Snowflake<UserMarker>, token: &Token) -> Result<(), TokenStoreError> {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account.get(), token.expose().to_owned());
        Ok(())
    }

    fn delete(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&account.get());
        Ok(())
    }
}

fn properties() -> ClientProperties {
    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "en-US".to_owned(),
    };
    ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs))
}

fn client_with(api: String, store: Arc<dyn TokenStore>) -> DiscordClient {
    let endpoints = Endpoints {
        api,
        allow_plaintext: true,
        ..Endpoints::default()
    };
    DiscordClient::with_endpoints(properties(), store, endpoints)
        .unwrap_or_else(|err| panic!("client setup failed: {err}"))
}

fn client(server: &MockServer, store: Arc<dyn TokenStore>) -> DiscordClient {
    client_with(format!("{}/api/v9/", server.uri()), store)
}

#[tokio::test]
async fn saved_tokens_load_back() {
    let server = MockServer::start().await;
    let client = client(&server, Arc::new(MemoryStore::default()));

    client
        .save_token(ACCOUNT, &Token::new("abc".to_owned()))
        .await
        .unwrap();

    let token = client.load_token(ACCOUNT).await.unwrap();
    assert_eq!(token.as_ref().map(Token::expose), Some("abc"));
    assert!(
        client
            .load_token(Snowflake::new(2))
            .await
            .unwrap()
            .is_none()
    );
}

struct BlockingStore {
    signal: Mutex<mpsc::Receiver<()>>,
}

impl TokenStore for BlockingStore {
    fn load(&self, _: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
        // Only arrives if another task can run while this call blocks.
        self.signal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| TokenStoreError::Backend("blocked the runtime".to_owned()))?;
        Ok(None)
    }

    fn save(&self, _: Snowflake<UserMarker>, _: &Token) -> Result<(), TokenStoreError> {
        Ok(())
    }

    fn delete(&self, _: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn store_calls_run_off_the_runtime_thread() {
    let server = MockServer::start().await;
    let (send, receive) = mpsc::channel();
    let store = BlockingStore {
        signal: Mutex::new(receive),
    };
    let client = client(&server, Arc::new(store));

    let signal = tokio::spawn(async move { send.send(()) });
    let loaded = client.load_token(ACCOUNT).await;

    assert!(loaded.is_ok(), "{loaded:?}");
    assert!(signal.await.unwrap().is_ok());
}

#[tokio::test]
async fn logout_ends_the_session_and_deletes_the_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/logout"))
        .and(header("authorization", "stored-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let store = Arc::new(MemoryStore::default());
    let client = client(&server, store.clone());
    client
        .save_token(ACCOUNT, &Token::new("stored-token".to_owned()))
        .await
        .unwrap();

    client.logout(ACCOUNT).await.unwrap();

    assert!(!store.holds(ACCOUNT));
}

#[tokio::test]
async fn logout_with_a_dead_token_still_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v9/auth/logout"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"message": "401: Unauthorized", "code": 0})),
        )
        .mount(&server)
        .await;
    let store = Arc::new(MemoryStore::default());
    let client = client(&server, store.clone());
    client
        .save_token(ACCOUNT, &Token::new("dead".to_owned()))
        .await
        .unwrap();

    client.logout(ACCOUNT).await.unwrap();

    assert!(!store.holds(ACCOUNT));
}

#[tokio::test]
async fn logout_offline_deletes_the_token_and_reports_the_network() {
    let store = Arc::new(MemoryStore::default());
    let client = client_with("http://127.0.0.1:1/api/v9/".to_owned(), store.clone());
    client
        .save_token(ACCOUNT, &Token::new("offline".to_owned()))
        .await
        .unwrap();

    let err = client.logout(ACCOUNT).await.unwrap_err();

    assert!(matches!(err, LogoutError::Network(_)), "{err:?}");
    assert!(!store.holds(ACCOUNT));
}

#[tokio::test]
async fn logout_without_a_token_is_not_logged_in() {
    let server = MockServer::start().await;
    let client = client(&server, Arc::new(MemoryStore::default()));

    let err = client.logout(ACCOUNT).await.unwrap_err();

    assert!(matches!(err, LogoutError::NotLoggedIn));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn end_session_logs_out_a_token_without_touching_the_store() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/logout"))
        .and(header("authorization", "replaced-token"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let store = Arc::new(MemoryStore::default());
    let client = client(&server, store.clone());
    client
        .save_token(ACCOUNT, &Token::new("current-token".to_owned()))
        .await
        .unwrap();

    client
        .end_session(&Token::new("replaced-token".to_owned()))
        .await
        .unwrap();

    assert!(store.holds(ACCOUNT));
}

#[tokio::test]
async fn ending_an_already_dead_session_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v9/auth/logout"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"message": "401: Unauthorized", "code": 0})),
        )
        .mount(&server)
        .await;
    let client = client(&server, Arc::new(MemoryStore::default()));

    assert!(
        client
            .end_session(&Token::new("dead".to_owned()))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn forget_token_deletes_without_a_request() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let store = Arc::new(MemoryStore::default());
    let client = client(&server, store.clone());
    client
        .save_token(ACCOUNT, &Token::new("rejected.token".to_owned()))
        .await
        .unwrap();

    client.forget_token(ACCOUNT).await.unwrap();

    assert!(!store.holds(ACCOUNT));
}
