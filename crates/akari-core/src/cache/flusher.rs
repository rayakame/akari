use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};

use super::{CacheThread, DiskCache, Snapshot};
use crate::state::Store;

enum Request {
    Flush(Option<oneshot::Sender<()>>),
    Close,
}

/// Writes an account's store to its cache: every interval while the store changes, and when
/// asked. The gateway's pump never waits for it.
pub(crate) struct Flusher {
    requests: mpsc::UnboundedSender<Request>,
}

impl Flusher {
    pub(crate) fn start(runtime: &tokio::runtime::Handle, store: Store, cache: DiskCache) -> Self {
        let (requests, receiver) = mpsc::unbounded_channel();
        runtime.spawn(run(store, cache.thread, cache.every, receiver));
        Self { requests }
    }

    /// Resolves once the store as it is now was written, or the write failed.
    pub(crate) async fn flush(&self) {
        let (reply, done) = oneshot::channel();
        if self.requests.send(Request::Flush(Some(reply))).is_ok() {
            let _ = done.await;
        }
    }

    pub(crate) fn flush_soon(&self) {
        let _ = self.requests.send(Request::Flush(None));
    }

    /// A last flush; nothing is written after it.
    pub(crate) fn close(&self) {
        let _ = self.requests.send(Request::Close);
    }
}

async fn run(
    store: Store,
    cache: CacheThread,
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
                    flush(&store, &cache, &mut flushed).await;
                }
            }
            request = requests.recv() => match request {
                Some(Request::Flush(reply)) => {
                    flush(&store, &cache, &mut flushed).await;
                    if let Some(reply) = reply {
                        let _ = reply.send(());
                    }
                }
                Some(Request::Close) | None => {
                    flush(&store, &cache, &mut flushed).await;
                    return;
                }
            },
        }
    }
}

// A failed write leaves `flushed` behind, so the next tick tries again.
async fn flush(store: &Store, cache: &CacheThread, flushed: &mut Option<u64>) {
    let changes = store.changes();
    if let Some(snapshot) = Snapshot::take(store)
        && cache.flush(snapshot).await.is_some()
    {
        *flushed = Some(changes);
    }
}
