use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::model::{ChannelId, MessageId};
use crate::random;
use crate::rest::AccountRest;
use crate::state::{Store, StoreEvent, Subscription};

// The documented limit per channel; the official client sends them all at once.
const CHUNK: usize = 75;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanTiming {
    pub(crate) wait: Duration,
    pub(crate) jitter: Duration,
}

// Discord's explicit content scans (D144): media in a viewed channel that waits for one is asked
// for once per session, per channel, after a jitter; the wait ends after `wait`, answered or not,
// as the official client gives up after 3 s.
pub(crate) async fn run_scans(
    store: Store,
    rest: Arc<AccountRest>,
    changes: Subscription,
    timing: ScanTiming,
) {
    loop {
        match changes.next().await {
            None => return,
            Some(event) if brings_media(&event) => {}
            Some(_) => continue,
        }
        for _ in 0..changes.buffered() {
            if changes.next().await.is_none() {
                return;
            }
        }
        for (channel, ids) in store.scans_due() {
            store.ask_scans(&ids);
            tokio::spawn(scan(store.clone(), rest.clone(), channel, ids, timing));
        }
    }
}

async fn scan(
    store: Store,
    rest: Arc<AccountRest>,
    channel: ChannelId,
    ids: Vec<MessageId>,
    timing: ScanTiming,
) {
    let asked = Instant::now();
    tokio::time::sleep(timing.jitter.mul_f64(random::unit())).await;
    for chunk in ids.chunks(CHUNK) {
        if let Err(failure) = rest.scan_explicit_media(channel, chunk).await {
            tracing::debug!(
                channel_id = channel.get(),
                messages = chunk.len(),
                ?failure,
                "explicit media scan not asked"
            );
        }
    }
    tokio::time::sleep_until(asked + timing.wait).await;
    store.settle_scans(channel, &ids);
}

// What can put media that waits for a scan in a window, or make waiting media of what's there.
fn brings_media(event: &StoreEvent) -> bool {
    matches!(
        event,
        StoreEvent::MessageInserted(_)
            | StoreEvent::MessageUpdated(_)
            | StoreEvent::MessagesLoaded { .. }
            | StoreEvent::MessagesRefreshed { .. }
            | StoreEvent::Restored
            | StoreEvent::Ready
            | StoreEvent::DisplaySettingsChanged
            | StoreEvent::CurrentUserUpdated(_)
    )
}
