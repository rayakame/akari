use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use akari_core::model::UserId;

use crate::client::Token;
use crate::{DiscordClient, Endpoints, HostInfo, TokenStore, TokenStoreError};

pub const USER: UserId = UserId::new(100_000_000_000_000_001);
// Nothing listens there, so connections are refused at once.
pub const UNREACHABLE: &str = "127.0.0.1:9";

pub fn token(value: &str) -> Arc<Token> {
    Token::new(akari_core::Token::new(value.to_owned()))
}

pub fn host() -> HostInfo {
    HostInfo {
        os_version: "25.0.0".to_owned(),
        system_locale: "en-US".to_owned(),
    }
}

pub fn unreachable_endpoints() -> Endpoints {
    Endpoints {
        api: format!("https://{UNREACHABLE}/api/v9/"),
        gateway: format!("wss://{UNREACHABLE}/"),
        remote_auth: format!("wss://{UNREACHABLE}/?v=2"),
        origin: "https://discord.com".to_owned(),
    }
}

// A client for local test servers, which `DiscordClient::with_endpoints` can't reach
// because they don't use TLS.
pub fn local_client(
    endpoints: akari_core::Endpoints,
    store: Arc<dyn TokenStore>,
) -> Arc<DiscordClient> {
    use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};

    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "en-US".to_owned(),
    };
    let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
    let endpoints = akari_core::Endpoints {
        allow_plaintext: true,
        ..endpoints
    };
    let core = akari_core::DiscordClient::with_endpoints(
        properties,
        Arc::new(crate::token_store::HostStore(store)),
        endpoints,
    )
    .unwrap_or_else(|err| panic!("client setup failed: {err}"));
    DiscordClient::from_core(core).unwrap_or_else(|err| panic!("{err}"))
}

// A REST server that already answers the fingerprint request logins start with.
pub async fn rest_server() -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    let server = wiremock::MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v9/experiments"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"fingerprint": "fp.1"})),
        )
        .mount(&server)
        .await;
    server
}

pub fn rest_endpoints(server: &wiremock::MockServer) -> akari_core::Endpoints {
    akari_core::Endpoints {
        api: format!("{}/api/v9/", server.uri()),
        ..akari_core::Endpoints::default()
    }
}

pub fn unreachable_client(store: Arc<dyn TokenStore>) -> Arc<DiscordClient> {
    DiscordClient::with_endpoints(host(), store, unreachable_endpoints())
        .unwrap_or_else(|err| panic!("client setup failed: {err}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Load(u64),
    Save(u64, String),
    Delete(u64),
}

#[derive(Default)]
pub struct MemoryStore {
    tokens: Mutex<HashMap<u64, String>>,
    calls: Mutex<Vec<Call>>,
    pub fail: Mutex<Option<TokenStoreError>>,
}

impl MemoryStore {
    pub fn with(account: UserId, token: &str) -> Arc<Self> {
        let store = Self::default();
        lock(&store.tokens).insert(account.get(), token.to_owned());
        Arc::new(store)
    }

    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    pub fn token(&self, account: UserId) -> Option<String> {
        lock(&self.tokens).get(&account.get()).cloned()
    }

    fn failure(&self) -> Result<(), TokenStoreError> {
        match &*lock(&self.fail) {
            Some(TokenStoreError::Backend { message }) => Err(TokenStoreError::Backend {
                message: message.clone(),
            }),
            Some(TokenStoreError::Unavailable) => Err(TokenStoreError::Unavailable),
            None => Ok(()),
        }
    }
}

impl TokenStore for MemoryStore {
    fn load(&self, account: UserId) -> Result<Option<String>, TokenStoreError> {
        lock(&self.calls).push(Call::Load(account.get()));
        self.failure()?;
        Ok(self.token(account))
    }

    fn save(&self, account: UserId, token: String) -> Result<(), TokenStoreError> {
        lock(&self.calls).push(Call::Save(account.get(), token.clone()));
        self.failure()?;
        lock(&self.tokens).insert(account.get(), token);
        Ok(())
    }

    fn delete(&self, account: UserId) -> Result<(), TokenStoreError> {
        lock(&self.calls).push(Call::Delete(account.get()));
        self.failure()?;
        lock(&self.tokens).remove(&account.get());
        Ok(())
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

// Polls `future` on this thread without any async runtime, the way UniFFI polls from
// Swift's threads.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        thread::park();
    }
}

// A token store whose `load` blocks until released, like a Keychain waiting on a dialog.
pub struct BlockingStore {
    entered: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl BlockingStore {
    // The store, a receiver that fires once `load` blocks, and the sender that releases it.
    pub fn new() -> (
        Arc<Self>,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered, on_entered) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        let store = Arc::new(Self {
            entered: Mutex::new(Some(entered)),
            release: Mutex::new(on_release),
        });
        (store, on_entered, release)
    }
}

impl TokenStore for BlockingStore {
    fn load(&self, _: UserId) -> Result<Option<String>, TokenStoreError> {
        if let Some(entered) = lock(&self.entered).take() {
            let _ = entered.send(());
        }
        let _ = lock(&self.release).recv();
        Ok(None)
    }

    fn save(&self, _: UserId, _: String) -> Result<(), TokenStoreError> {
        Ok(())
    }

    fn delete(&self, _: UserId) -> Result<(), TokenStoreError> {
        Ok(())
    }
}
