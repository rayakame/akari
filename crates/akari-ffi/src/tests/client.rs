use std::sync::Arc;
use std::time::Duration;

use akari_core::model::UserId;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{
    Call, MemoryStore, USER, block_on, host, local_client, rest_endpoints, unreachable_client,
    unreachable_endpoints,
};
use crate::client::{DiscordClient, Token};
use crate::errors::{ClientError, LogoutError, NetworkErrorKind, TokenStoreError};
use crate::runtime::{run, runtime};
use crate::{Endpoints, enable_logging};

fn token(value: &str) -> Arc<Token> {
    Token::new(akari_core::Token::new(value.to_owned()))
}

#[test]
fn plaintext_and_malformed_endpoints_are_rejected() {
    let store = Arc::new(MemoryStore::default());
    let plaintext = Endpoints {
        api: "http://127.0.0.1:9/api/v9/".to_owned(),
        ..unreachable_endpoints()
    };
    let malformed = Endpoints {
        gateway: "not a url".to_owned(),
        ..unreachable_endpoints()
    };

    let plaintext = DiscordClient::with_endpoints(host(), store.clone(), plaintext);
    let malformed = DiscordClient::with_endpoints(host(), store, malformed);

    assert!(
        matches!(&plaintext, Err(ClientError::InvalidEndpoint { name }) if name == "api"),
        "{:?}",
        plaintext.err()
    );
    assert!(
        matches!(&malformed, Err(ClientError::InvalidEndpoint { name }) if name == "gateway"),
        "{:?}",
        malformed.err()
    );
}

#[test]
fn every_client_shares_one_runtime() {
    let first = unreachable_client(Arc::new(MemoryStore::default()));
    let second = unreachable_client(Arc::new(MemoryStore::default()));

    assert!(std::ptr::eq(first.runtime(), second.runtime()));
}

#[test]
fn tokens_round_trip_through_the_host_store() {
    let store = Arc::new(MemoryStore::default());
    let client = unreachable_client(store.clone());

    block_on(client.save_token(USER, token("first.token"))).unwrap();
    let loaded = block_on(client.load_token(USER)).unwrap().unwrap();
    block_on(client.save_token(UserId::new(7), loaded)).unwrap();

    assert_eq!(store.token(USER).as_deref(), Some("first.token"));
    assert_eq!(store.token(UserId::new(7)).as_deref(), Some("first.token"));
    assert_eq!(
        block_on(client.load_token(UserId::new(8)))
            .unwrap()
            .map(|_| ()),
        None
    );
}

#[test]
fn host_store_errors_keep_their_kind() {
    let store = Arc::new(MemoryStore::default());
    let client = unreachable_client(store.clone());

    *super::support::lock(&store.fail) = Some(TokenStoreError::Unavailable);
    let unavailable = block_on(client.load_token(USER)).map(|_| ());
    *super::support::lock(&store.fail) = Some(TokenStoreError::Backend {
        message: "keychain said no".to_owned(),
    });
    let backend = block_on(client.save_token(USER, token("t"))).map(|_| ());
    let unexpected = TokenStoreError::from(uniffi::UnexpectedUniFFICallbackError::new("boom"));

    assert_eq!(unavailable, Err(TokenStoreError::Unavailable));
    assert_eq!(
        backend,
        Err(TokenStoreError::Backend {
            message: "keychain said no".to_owned()
        })
    );
    assert!(
        matches!(&unexpected, TokenStoreError::Backend { message } if message.contains("boom")),
        "{unexpected:?}"
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
    let store = MemoryStore::with(USER, "rejected.token");
    let client = local_client(rest_endpoints(&server), store.clone());

    client.forget_token(USER).await.unwrap();

    assert_eq!(store.token(USER), None);
    assert_eq!(store.calls(), [Call::Delete(USER.get())]);
}

#[test]
fn logout_deletes_the_token_when_discord_is_unreachable() {
    let store = MemoryStore::with(USER, "stored.token");
    let client = unreachable_client(store.clone());

    let result = block_on(client.logout(USER));

    assert_eq!(
        result,
        Err(LogoutError::Network {
            kind: NetworkErrorKind::Connect
        })
    );
    assert_eq!(store.token(USER), None);
    assert!(store.calls().contains(&Call::Delete(USER.get())));
}

#[test]
fn async_methods_work_without_a_tokio_context() {
    let store = MemoryStore::with(USER, "stored.token");
    let client = unreachable_client(store.clone());

    let loaded = std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let loaded = block_on(client.load_token(USER)).map(|token| token.is_some());
        let logout = block_on(client.logout(USER));
        (loaded, logout)
    })
    .join()
    .unwrap();

    assert_eq!(loaded.0, Ok(true));
    assert!(
        matches!(loaded.1, Err(LogoutError::Network { .. })),
        "{:?}",
        loaded.1
    );
}

#[test]
fn a_panic_in_run_reaches_the_awaiting_caller() {
    let runtime = runtime().unwrap();

    let caught = std::panic::catch_unwind(|| {
        block_on(run(runtime, async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            panic!("boom in the task");
        }))
    });

    let payload = caught.unwrap_err();
    let message = payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned());
    assert_eq!(message.as_deref(), Some("boom in the task"));
}

#[test]
fn enable_logging_rejects_a_bad_filter() {
    assert_eq!(
        enable_logging("akari_core=nonsense[".to_owned()),
        Err(ClientError::InvalidLogFilter)
    );
}
