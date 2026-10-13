use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::time::timeout;

use super::fake::FakeGateway;
use super::gateway::WAIT;
use super::support::{MemoryStore, USER, core_client, host, token};
use crate::client::DiscordClient;
use crate::subscription::{StoreEvent, StoreSubscription};

pub(super) struct TempDir(PathBuf);

impl TempDir {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "akari-ffi-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn path(&self) -> PathBuf {
        self.0.clone()
    }

    fn file(&self) -> PathBuf {
        self.0.join(format!("{}.sqlite", USER.get()))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn wait_for(condition: impl Fn() -> bool) {
    timeout(WAIT, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("it didn't happen");
}

async fn until(subscription: &StoreSubscription, event: &StoreEvent) {
    timeout(WAIT, async {
        while let Some(events) = subscription.next().await {
            if events.contains(event) {
                return;
            }
        }
        panic!("the subscription ended before {event:?}");
    })
    .await
    .unwrap_or_else(|_| panic!("no {event:?}"));
}

#[tokio::test]
async fn the_constructor_takes_the_cache_directory() {
    let dir = TempDir::new();
    let client = DiscordClient::new(
        host(),
        Arc::new(MemoryStore::default()),
        Some(dir.0.to_string_lossy().into_owned()),
    )
    .unwrap();

    client.preload(USER);

    wait_for(|| dir.file().exists()).await;
}

#[tokio::test]
async fn no_directory_means_no_cache() {
    let dir = TempDir::new();
    let client = DiscordClient::new(host(), Arc::new(MemoryStore::default()), None).unwrap();

    client.preload(USER);
    client.clear_caches().await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[tokio::test]
async fn caches_cross_the_boundary() {
    let dir = TempDir::new();
    let gateway = FakeGateway::start().await;
    let core = core_client(gateway.endpoints(), Arc::new(MemoryStore::default()))
        .with_cache(dir.0.clone());
    let client = DiscordClient::from_core(core).unwrap();
    let account = client.account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    until(&subscription, &StoreEvent::Ready).await;

    account.flush_cache().await;
    account.close();
    drop(account);

    assert!(dir.file().exists());
    let again = client.account(token("t"), USER).unwrap();
    let restored = again.store().subscribe();
    // As a host does: a restore that landed before the subscription sends no event to it.
    if again.store().current_user().is_none() {
        until(&restored, &StoreEvent::Restored).await;
    }
    assert_eq!(again.store().guild_ids().len(), 1);

    client.forget_account(USER).await.unwrap();

    assert!(!dir.file().exists());
}
