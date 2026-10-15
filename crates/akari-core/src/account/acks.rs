use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::mutes::until;
use crate::model::{ChannelId, MessageId};
use crate::ms;
use crate::rest::{AccountRest, RequestError, last_viewed};
use crate::state::wall_millis;

const TARGET: &str = "akari_core::acks";
// Like the official client: an ack about 2 s after the last message read.
const LULL: Duration = Duration::from_secs(2);
// A busy channel still reaches the user's other devices.
const CAP: Duration = Duration::from_secs(10);
const BULK_CHUNK: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckTarget {
    // A real message ID: the channel's newest message.
    Message(MessageId),
    // A timestamp-only snowflake for Discord's "now", never below `floor`.
    Now { floor: Option<MessageId> },
}

impl AckTarget {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Message(a), Self::Message(b)) => Self::Message(a.max(b)),
            (Self::Message(id), Self::Now { floor }) | (Self::Now { floor }, Self::Message(id)) => {
                Self::Now {
                    floor: floor.max(Some(id)),
                }
            }
            (Self::Now { floor: a }, Self::Now { floor: b }) => Self::Now { floor: a.max(b) },
        }
    }

    fn resolve(self, server_now: Option<i64>) -> Option<MessageId> {
        match self {
            Self::Message(id) => Some(id),
            Self::Now { floor } => server_now
                .map(|now| MessageId::from_unix_millis(now, 0))
                .max(floor),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pace {
    Paced,
    Explicit,
}

impl Pace {
    fn name(self) -> &'static str {
        match self {
            Self::Paced => "paced",
            Self::Explicit => "explicit",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Paced { last: Instant },
    Explicit { at: Instant },
    Bulk { at: Instant },
}

#[derive(Debug)]
struct Wanted {
    target: AckTarget,
    kind: Kind,
    // When the channel's first pending target came.
    since: Instant,
    order: u64,
}

impl Wanted {
    fn due(&self) -> Instant {
        match self.kind {
            Kind::Paced { last } => (last + LULL).min(self.since + CAP),
            Kind::Explicit { at } | Kind::Bulk { at } => at,
        }
    }

    fn bulk(&self) -> bool {
        matches!(self.kind, Kind::Bulk { .. })
    }
}

#[derive(Debug)]
struct Request {
    entries: Vec<(ChannelId, AckTarget)>,
    bulk: bool,
    pace: Pace,
    waited: Duration,
}

enum Next {
    Send(Request),
    At(Instant),
    Idle,
}

// At most one pending target per channel.
#[derive(Default)]
struct Queue {
    wanted: HashMap<ChannelId, Wanted>,
    orders: u64,
    closed: bool,
}

impl Queue {
    fn order(&mut self) -> u64 {
        self.orders += 1;
        self.orders
    }

    fn want(&mut self, channel: ChannelId, target: AckTarget, pace: Pace, now: Instant) {
        if self.closed {
            return;
        }
        let order = self.order();
        match self.wanted.entry(channel) {
            Entry::Vacant(entry) => {
                let kind = match pace {
                    Pace::Paced => Kind::Paced { last: now },
                    Pace::Explicit => Kind::Explicit { at: now },
                };
                entry.insert(Wanted {
                    target,
                    kind,
                    since: now,
                    order,
                });
            }
            Entry::Occupied(mut entry) => {
                let wanted = entry.get_mut();
                wanted.target = wanted.target.merge(target);
                wanted.kind = match (wanted.kind, pace) {
                    (Kind::Paced { .. }, Pace::Paced) => Kind::Paced { last: now },
                    (Kind::Paced { .. }, Pace::Explicit) => Kind::Explicit {
                        at: now.min(wanted.due()),
                    },
                    (kind, _) => kind,
                };
            }
        }
    }

    fn want_bulk(&mut self, entries: Vec<(ChannelId, AckTarget)>, now: Instant) {
        if self.closed {
            return;
        }
        for (channel, target) in entries {
            let order = self.order();
            match self.wanted.entry(channel) {
                Entry::Vacant(entry) => {
                    entry.insert(Wanted {
                        target,
                        kind: Kind::Bulk { at: now },
                        since: now,
                        order,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let wanted = entry.get_mut();
                    wanted.target = wanted.target.merge(target);
                    if !wanted.bulk() {
                        wanted.kind = Kind::Bulk { at: now };
                        wanted.order = order;
                    }
                }
            }
        }
    }

    // Oldest due first; a request takes the bulk entries due in a row, up to a chunk.
    fn take(&mut self, now: Instant) -> Next {
        let Some((&channel, head)) = self
            .wanted
            .iter()
            .min_by_key(|(_, wanted)| (wanted.due(), wanted.order))
        else {
            return Next::Idle;
        };
        if head.due() > now {
            return Next::At(head.due());
        }
        let channels = if head.bulk() {
            let mut due: Vec<(Instant, u64, ChannelId, bool)> = self
                .wanted
                .iter()
                .filter(|(_, wanted)| wanted.due() <= now)
                .map(|(channel, wanted)| (wanted.due(), wanted.order, *channel, wanted.bulk()))
                .collect();
            due.sort_unstable();
            due.into_iter()
                .take_while(|(.., bulk)| *bulk)
                .take(BULK_CHUNK)
                .map(|(_, _, channel, _)| channel)
                .collect()
        } else {
            vec![channel]
        };
        let mut request = Request {
            entries: Vec::with_capacity(channels.len()),
            bulk: head.bulk(),
            pace: match head.kind {
                Kind::Paced { .. } => Pace::Paced,
                Kind::Explicit { .. } | Kind::Bulk { .. } => Pace::Explicit,
            },
            waited: Duration::ZERO,
        };
        for channel in channels {
            if let Some(wanted) = self.wanted.remove(&channel) {
                request.waited = request
                    .waited
                    .max(now.saturating_duration_since(wanted.since));
                request.entries.push((channel, wanted.target));
            }
        }
        Next::Send(request)
    }

    fn close(&mut self) {
        self.closed = true;
        self.wanted = HashMap::new();
    }
}

#[derive(Default)]
struct Wants {
    queue: Mutex<Queue>,
    changed: Notify,
}

impl Wants {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn want(&self, channel: ChannelId, target: AckTarget, pace: Pace) {
        self.lock().want(channel, target, pace, Instant::now());
        self.changed.notify_one();
    }

    fn want_bulk(&self, entries: Vec<(ChannelId, AckTarget)>) {
        self.lock().want_bulk(entries, Instant::now());
        self.changed.notify_one();
    }

    fn close(&self) {
        self.lock().close();
        self.changed.notify_one();
    }

    fn closed(&self) -> bool {
        self.lock().closed
    }

    // None once closed.
    async fn next(&self) -> Option<Request> {
        loop {
            let next = {
                let mut queue = self.lock();
                if queue.closed {
                    return None;
                }
                queue.take(Instant::now())
            };
            let wake = match next {
                Next::Send(request) => return Some(request),
                Next::At(at) => Some(at),
                Next::Idle => None,
            };
            tokio::select! {
                () = self.changed.notified() => {}
                () = until(wake) => {}
            }
        }
    }
}

// Closing or dropping it drops what's pending; a request already on its way isn't cancelled.
pub(crate) struct AckSender {
    wants: Arc<Wants>,
}

impl AckSender {
    pub(crate) fn start(
        runtime: &tokio::runtime::Handle,
        rest: Arc<AccountRest>,
        retry_delay: Duration,
    ) -> Self {
        let wants = Arc::new(Wants::default());
        runtime.spawn(send(wants.clone(), rest, retry_delay));
        Self { wants }
    }

    // Returns at once; a pending target for the channel merges with this one.
    #[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests yet"))]
    pub(crate) fn want(&self, channel: ChannelId, target: AckTarget, pace: Pace) {
        self.wants.want(channel, target, pace);
    }

    // Explicit; takes over the channels' pending single acks.
    #[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests yet"))]
    pub(crate) fn want_bulk(&self, entries: Vec<(ChannelId, AckTarget)>) {
        self.wants.want_bulk(entries);
    }

    pub(crate) fn close(&self) {
        self.wants.close();
    }
}

impl Drop for AckSender {
    fn drop(&mut self) {
        self.close();
    }
}

async fn send(wants: Arc<Wants>, rest: Arc<AccountRest>, retry_delay: Duration) {
    // Per channel, the highest ID Discord took this session.
    let mut sent: HashMap<ChannelId, MessageId> = HashMap::new();
    while let Some(request) = wants.next().await {
        let server_now = rest.server_now_millis();
        let entries: Vec<(ChannelId, MessageId)> = request
            .entries
            .iter()
            .filter_map(|&(channel, target)| {
                let Some(id) = target.resolve(server_now) else {
                    tracing::debug!(
                        target: TARGET,
                        channel_id = channel.get(),
                        "no ack: neither Discord's time nor a message is known"
                    );
                    return None;
                };
                sent.get(&channel)
                    .is_none_or(|last| id > *last)
                    .then_some((channel, id))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        let last_viewed = last_viewed(server_now.unwrap_or_else(wall_millis));
        let mut retried = false;
        let result = loop {
            let result = match entries[..] {
                [(channel, id)] if !request.bulk => rest.ack(channel, id, last_viewed).await,
                _ => rest.ack_bulk(&entries).await,
            };
            match result {
                Err(err) if err.is_transient() && !retried => {
                    log(&request, &entries, Outcome::Retrying(&err));
                    retried = true;
                    tokio::time::sleep(retry_delay).await;
                    if wants.closed() {
                        return;
                    }
                }
                result => break result,
            }
        };
        match result {
            Ok(()) => {
                sent.extend(entries.iter().copied());
                log(&request, &entries, Outcome::Sent);
            }
            Err(err) => {
                log(&request, &entries, Outcome::Dropped(&err));
                if matches!(err, RequestError::Unauthorized | RequestError::Closed) {
                    wants.close();
                    return;
                }
            }
        }
    }
}

enum Outcome<'a> {
    Sent,
    Retrying(&'a RequestError),
    Dropped(&'a RequestError),
}

fn log(request: &Request, entries: &[(ChannelId, MessageId)], outcome: Outcome<'_>) {
    let (channel_id, message_id) = match entries {
        [(channel, id)] if !request.bulk => (Some(channel.get()), Some(id.get())),
        _ => (None, None),
    };
    let entries = request.bulk.then_some(entries.len());
    let pace = request.pace.name();
    let waited_ms = ms(request.waited);
    match outcome {
        Outcome::Sent => tracing::debug!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            pace,
            waited_ms,
            "ack sent"
        ),
        Outcome::Retrying(err) => tracing::debug!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            pace,
            waited_ms,
            error = err.kind(),
            "ack failed; retrying"
        ),
        Outcome::Dropped(RequestError::Closed) => tracing::debug!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            "ack dropped: the account closed"
        ),
        Outcome::Dropped(err) => tracing::warn!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            pace,
            waited_ms,
            error = err.kind(),
            "ack dropped"
        ),
    }
}

#[cfg(test)]
mod tests;
