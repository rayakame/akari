use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use akari_core::model::UserId;

use crate::{DiscordClient, Endpoints, HostInfo, TokenStore, TokenStoreError};

pub const USER: UserId = UserId::new(100_000_000_000_000_001);
/// Nothing listens there, so connections are refused at once.
pub const UNREACHABLE: &str = "127.0.0.1:9";

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

/// A token store in memory that records its calls; `fail` makes every call fail.
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

/// Polls `future` on this thread without any async runtime, the way UniFFI polls from
/// Swift's threads.
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
