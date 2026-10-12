use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};

use super::{CacheThread, Caches, DiskCache, LOG, Snapshot};
use crate::model::UserId;
use crate::state::Store;

enum Request {
    Flush(Option<oneshot::Sender<()>>),
    Close,
    Delete,
}

// The gateway's pump never waits for it: the pump only bumps the store's change counter.
pub(crate) struct Flusher {
    requests: mpsc::UnboundedSender<Request>,
}

impl Flusher {
    pub(crate) fn start(runtime: &tokio::runtime::Handle, store: Store, cache: DiskCache) -> Self {
        let (requests, receiver) = mpsc::unbounded_channel();
        let every = cache.every;
        runtime.spawn(run(
            store,
            Target {
                thread: cache.thread,
                user: cache.user,
                caches: cache.caches,
            },
            every,
            receiver,
        ));
        Self { requests }
    }

    pub(crate) async fn flush(&self) {
        let (reply, done) = oneshot::channel();
        if self.requests.send(Request::Flush(Some(reply))).is_ok() {
            let _ = done.await;
        }
    }

    pub(crate) fn flush_soon(&self) {
        let _ = self.requests.send(Request::Flush(None));
    }

    pub(crate) fn close(&self) {
        let _ = self.requests.send(Request::Close);
    }

    pub(crate) fn delete(&self) {
        let _ = self.requests.send(Request::Delete);
    }
}

struct Target {
    thread: CacheThread,
    user: UserId,
    caches: Option<Arc<Caches>>,
}

async fn run(
    store: Store,
    mut target: Target,
    every: Duration,
    mut requests: mpsc::UnboundedReceiver<Request>,
) {
    let mut flushed = None;
    let mut ticks = tokio::time::interval_at(Instant::now() + every, every);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {
                if flushed != Some(store.changes()) {
                    flush(&store, &mut target, &mut flushed).await;
                }
            }
            request = requests.recv() => match request {
                Some(Request::Flush(reply)) => {
                    flush(&store, &mut target, &mut flushed).await;
                    if let Some(reply) = reply {
                        let _ = reply.send(());
                    }
                }
                Some(Request::Close) | None => {
                    flush(&store, &mut target, &mut flushed).await;
                    return;
                }
                Some(Request::Delete) => {
                    match &target.caches {
                        Some(caches) => caches.forget(target.user).await,
                        None => target.thread.delete().await,
                    }
                    return;
                }
            },
        }
    }
}

// A failed write leaves `flushed` behind, so the next tick tries again.
async fn flush(store: &Store, target: &mut Target, flushed: &mut Option<u64>) {
    let changes = store.changes();
    let Some(snapshot) = Snapshot::take(store) else {
        return;
    };
    // The host named another account than READY's: that file stays for its own account.
    if let Some(session) = snapshot
        .entities
        .current_user
        .as_ref()
        .map(|current| current.user.id)
        && session != target.user
        && let Some(caches) = &target.caches
    {
        let Some(thread) = caches.thread(session) else {
            return;
        };
        tracing::info!(target: LOG, "cache moved to the session's account");
        target.thread = thread;
        target.user = session;
    }
    if target.thread.flush(snapshot).await.is_some() {
        *flushed = Some(changes);
    }
}
