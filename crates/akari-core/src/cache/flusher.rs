use std::sync::{Arc, Mutex, PoisonError};
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
    source: Arc<Source>,
    #[cfg(test)]
    paused: Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>,
}

// Snapshots end with close's, so nothing that changes after close() returns is written.
struct Source {
    store: Store,
    closing: Mutex<Closing>,
}

#[derive(Default)]
struct Closing {
    closed: bool,
    // Kept here, not in the request: another close's request can reach the task first.
    last: Option<Snapshot>,
}

impl Source {
    fn closing(&self) -> std::sync::MutexGuard<'_, Closing> {
        self.closing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Under the lock, so none is taken after close's.
    fn snapshot(&self) -> Option<Snapshot> {
        let closing = self.closing();
        if closing.closed {
            return None;
        }
        Snapshot::take(&self.store)
    }

    fn close(&self) {
        let mut closing = self.closing();
        if !closing.closed {
            closing.closed = true;
            closing.last = Snapshot::take(&self.store).map(|snapshot| Snapshot {
                last: true,
                ..snapshot
            });
        }
    }

    fn take_last(&self) -> Option<Snapshot> {
        self.closing().last.take()
    }
}

impl Flusher {
    pub(crate) fn start(runtime: &tokio::runtime::Handle, store: Store, cache: DiskCache) -> Self {
        let (requests, receiver) = mpsc::unbounded_channel();
        let every = cache.every;
        let source = Arc::new(Source {
            store,
            closing: Mutex::default(),
        });
        runtime.spawn(run(
            source.clone(),
            Target {
                thread: cache.thread,
                user: cache.user,
                caches: cache.caches,
            },
            every,
            receiver,
        ));
        Self {
            requests,
            source,
            #[cfg(test)]
            paused: Mutex::default(),
        }
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
        self.source.close();
        #[cfg(test)]
        self.pause();
        let _ = self.requests.send(Request::Close);
    }

    // The next close() stops between its snapshot and its request until released.
    #[cfg(test)]
    pub(crate) fn pause_next_close(
        &self,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (reached, reaching) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        *self.paused.lock().unwrap_or_else(PoisonError::into_inner) = Some((reached, released));
        (reaching, release)
    }

    #[cfg(test)]
    fn pause(&self) {
        let paused = self
            .paused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some((reached, release)) = paused {
            let _ = reached.send(());
            let _ = release.recv();
        }
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
    source: Arc<Source>,
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
                if flushed != Some(source.store.changes()) {
                    flush(&source, &mut target, &mut flushed).await;
                }
            }
            request = requests.recv() => match request {
                Some(Request::Flush(reply)) => {
                    flush(&source, &mut target, &mut flushed).await;
                    if let Some(reply) = reply {
                        let _ = reply.send(());
                    }
                }
                Some(Request::Close) => {
                    if let Some(last) = source.take_last() {
                        write(&mut target, last).await;
                    }
                    return;
                }
                None => {
                    flush(&source, &mut target, &mut flushed).await;
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
async fn flush(source: &Source, target: &mut Target, flushed: &mut Option<u64>) {
    let changes = source.store.changes();
    let Some(snapshot) = source.snapshot() else {
        return;
    };
    if write(target, snapshot).await {
        *flushed = Some(changes);
    }
}

async fn write(target: &mut Target, snapshot: Snapshot) -> bool {
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
            return false;
        };
        tracing::info!(target: LOG, "cache moved to the session's account");
        target.thread = thread;
        target.user = session;
    }
    target.thread.flush(snapshot).await.is_some()
}
