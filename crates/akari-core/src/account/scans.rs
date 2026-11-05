use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::model::{ChannelId, MessageId};
use crate::random;
use crate::rest::{AccountRest, RequestError};
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
// as the official client gives up after 3 s. An ask that failed for a while goes again on a
// reload.
pub(crate) async fn run_scans(
    store: Store,
    rest: Arc<AccountRest>,
    changes: Subscription,
    timing: ScanTiming,
) {
    loop {
        let Some(event) = changes.next().await else {
            return;
        };
        let mut due = brings_media(&event);
        let mut retry = reloads(&event);
        for _ in 0..changes.buffered() {
            let Some(event) = changes.next().await else {
                return;
            };
            due |= brings_media(&event);
            retry |= reloads(&event);
        }
        if !due {
            continue;
        }
        for (channel, ids) in store.scans_due(retry) {
            store.ask_scans(&ids);
            let ends = Instant::now() + timing.wait;
            tokio::spawn(settle(store.clone(), channel, ids.clone(), ends));
            tokio::spawn(ask(
                store.clone(),
                rest.clone(),
                channel,
                ids,
                timing.jitter,
            ));
        }
    }
}

// However long the request takes (its timeouts are 10 and 30 s).
async fn settle(store: Store, channel: ChannelId, ids: Vec<MessageId>, ends: Instant) {
    tokio::time::sleep_until(ends).await;
    store.settle_scans(channel, &ids);
}

async fn ask(
    store: Store,
    rest: Arc<AccountRest>,
    channel: ChannelId,
    ids: Vec<MessageId>,
    jitter: Duration,
) {
    tokio::time::sleep(jitter.mul_f64(random::unit())).await;
    for chunk in ids.chunks(CHUNK) {
        if let Err(failure) = rest.scan_explicit_media(channel, chunk).await {
            let error = RequestError::from(failure);
            tracing::debug!(
                channel_id = channel.get(),
                messages = chunk.len(),
                error = error.kind(),
                "explicit media scan not asked"
            );
            // A refusal would be refused again.
            if error.is_transient() || matches!(error, RequestError::RateLimited { .. }) {
                store.fail_scans(chunk);
            }
        }
    }
}

fn reloads(event: &StoreEvent) -> bool {
    matches!(
        event,
        StoreEvent::Ready
            | StoreEvent::MessagesLoaded { .. }
            | StoreEvent::MessagesRefreshed { .. }
    )
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
