use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::mutes::until;
use crate::model::{ChannelId, MessageId};
use crate::ms;
use crate::rest::{AccountRest, RequestError, last_viewed};
use crate::state::{Ack, wall_millis};

const TARGET: &str = "akari_core::acks";
// Like the official client: an ack about 2 s after the last message read.
const LULL: Duration = Duration::from_secs(2);
// A busy channel still reaches the user's other devices.
const CAP: Duration = Duration::from_secs(10);
const BULK_CHUNK: usize = 50;
// A failed ack's wait doubles from the first retry's up to this.
const RETRY_CAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckTarget {
    // A real message ID: the channel's newest message.
    Message(MessageId),
    // A timestamp-only snowflake for Discord's "now", never below `floor`.
    Now { floor: Option<MessageId> },
}

impl From<Ack> for AckTarget {
    fn from(ack: Ack) -> Self {
        match ack.cursor {
            Some(cursor) if !ack.now => Self::Message(cursor),
            floor => Self::Now { floor },
        }
    }
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

#[derive(Debug, Clone, Copy)]
struct Retry {
    at: Instant,
    // Failed requests in a row.
    failures: u32,
}

#[derive(Debug)]
struct Wanted {
    target: AckTarget,
    kind: Kind,
    // When the channel's first pending target came.
    since: Instant,
    order: u64,
    retry: Option<Retry>,
}

impl Wanted {
    fn due(&self) -> Instant {
        let due = match self.kind {
            Kind::Paced { last } => (last + LULL).min(self.since + CAP),
            Kind::Explicit { at } | Kind::Bulk { at } => at,
        };
        self.retry.map_or(due, |retry| due.max(retry.at))
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
    since: Instant,
    failures: u32,
}

impl Request {
    fn kind(&self) -> Kind {
        match (self.bulk, self.pace) {
            (true, _) => Kind::Bulk { at: self.since },
            (false, Pace::Paced) => Kind::Paced { last: self.since },
            (false, Pace::Explicit) => Kind::Explicit { at: self.since },
        }
    }
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
    // Channels whose position Discord set on its own, for the sender to forget.
    forgotten: HashSet<ChannelId>,
    forget_all: bool,
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
                    retry: None,
                });
            }
            Entry::Occupied(mut entry) => {
                let wanted = entry.get_mut();
                let merged = wanted.target.merge(target);
                let advanced = merged != wanted.target;
                wanted.target = merged;
                wanted.kind = match (wanted.kind, pace) {
                    (Kind::Paced { .. }, Pace::Paced) if advanced => Kind::Paced { last: now },
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
                        retry: None,
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
            since: head.since,
            failures: 0,
        };
        for channel in channels {
            if let Some(wanted) = self.wanted.remove(&channel) {
                request.waited = request
                    .waited
                    .max(now.saturating_duration_since(wanted.since));
                request.since = request.since.min(wanted.since);
                request.failures = request
                    .failures
                    .max(wanted.retry.map_or(0, |retry| retry.failures));
                request.entries.push((channel, wanted.target));
            }
        }
        Next::Send(request)
    }

    // A failed request's targets wait until `at`, merged with what came for their channels since.
    fn retry(&mut self, request: Request, at: Instant) {
        if self.closed {
            return;
        }
        let kind = request.kind();
        let retry = Retry {
            at,
            failures: request.failures.saturating_add(1),
        };
        for (channel, target) in request.entries {
            let order = self.order();
            match self.wanted.entry(channel) {
                Entry::Vacant(entry) => {
                    entry.insert(Wanted {
                        target,
                        kind,
                        since: request.since,
                        order,
                        retry: Some(retry),
                    });
                }
                Entry::Occupied(mut entry) => {
                    let wanted = entry.get_mut();
                    wanted.target = wanted.target.merge(target);
                    wanted.since = wanted.since.min(request.since);
                    wanted.retry = Some(retry);
                }
            }
        }
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

    fn retry(&self, request: Request, at: Instant) {
        self.lock().retry(request, at);
    }

    fn forget(&self, channel: ChannelId) {
        self.lock().forgotten.insert(channel);
    }

    fn forget_all(&self) {
        self.lock().forget_all = true;
    }

    // Taken between requests, so a request on its way when Discord moved can't undo it.
    fn take_forgotten(&self, sent: &mut HashMap<ChannelId, MessageId>) {
        let mut queue = self.lock();
        if std::mem::take(&mut queue.forget_all) {
            sent.clear();
        }
        for channel in queue.forgotten.drain() {
            sent.remove(&channel);
        }
    }

    fn close(&self) {
        self.lock().close();
        self.changed.notify_one();
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

// What the account handed to the sender, as tests see it.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Handed {
    One(ChannelId, AckTarget, Pace),
    Bulk(Vec<(ChannelId, AckTarget)>),
}

// Closing or dropping it drops what's pending; a request already on its way isn't cancelled.
pub(crate) struct AckSender {
    wants: Arc<Wants>,
    #[cfg(test)]
    handed: Mutex<Vec<Handed>>,
}

impl AckSender {
    // `unauthorized` runs once if Discord answers an ack with 401; the sender has stopped then.
    // A failed ack is retried after `retry_delay`, then after twice as long each time, up to a
    // minute.
    pub(crate) fn start(
        runtime: &tokio::runtime::Handle,
        rest: Arc<AccountRest>,
        retry_delay: Duration,
        unauthorized: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let wants = Arc::new(Wants::default());
        runtime.spawn(send(
            wants.clone(),
            rest,
            retry_delay,
            Box::new(unauthorized),
        ));
        Self {
            wants,
            #[cfg(test)]
            handed: Mutex::default(),
        }
    }

    // Returns at once; a pending target for the channel merges with this one.
    pub(crate) fn want(&self, channel: ChannelId, target: AckTarget, pace: Pace) {
        #[cfg(test)]
        self.hand(Handed::One(channel, target, pace));
        self.wants.want(channel, target, pace);
    }

    // Explicit; takes over the channels' pending single acks.
    pub(crate) fn want_bulk(&self, entries: Vec<(ChannelId, AckTarget)>) {
        #[cfg(test)]
        self.hand(Handed::Bulk(entries.clone()));
        self.wants.want_bulk(entries);
    }

    // Discord set the channel's position on its own, maybe behind what was sent (a mark as
    // unread elsewhere): the next target goes out even if it isn't newer.
    pub(crate) fn forget(&self, channel: ChannelId) {
        self.wants.forget(channel);
    }

    // READY's read states are Discord's, whatever was sent before.
    pub(crate) fn forget_all(&self) {
        self.wants.forget_all();
    }

    pub(crate) fn close(&self) {
        self.wants.close();
    }

    #[cfg(test)]
    fn hand(&self, handed: Handed) {
        self.handed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(handed);
    }

    #[cfg(test)]
    pub(crate) fn handed(&self) -> Vec<Handed> {
        self.handed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for AckSender {
    fn drop(&mut self) {
        self.close();
    }
}

async fn send(
    wants: Arc<Wants>,
    rest: Arc<AccountRest>,
    retry_delay: Duration,
    unauthorized: Box<dyn Fn() + Send + Sync>,
) {
    // Per channel, the highest ID Discord took since it last set the position on its own.
    let mut sent: HashMap<ChannelId, MessageId> = HashMap::new();
    while let Some(mut request) = wants.next().await {
        wants.take_forgotten(&mut sent);
        let server_now = rest.server_now_millis();
        let mut entries: Vec<(ChannelId, MessageId)> = Vec::with_capacity(request.entries.len());
        request.entries.retain(|&(channel, target)| {
            let Some(id) = target.resolve(server_now) else {
                tracing::debug!(
                    target: TARGET,
                    channel_id = channel.get(),
                    "no ack: neither Discord's time nor a message is known"
                );
                return false;
            };
            let new = sent.get(&channel).is_none_or(|last| id > *last);
            if new {
                entries.push((channel, id));
            }
            new
        });
        if entries.is_empty() {
            continue;
        }
        let last_viewed = last_viewed(server_now.unwrap_or_else(wall_millis));
        let result = match entries[..] {
            [(channel, id)] if !request.bulk => rest.ack(channel, id, last_viewed).await,
            _ => rest.ack_bulk(&entries).await,
        };
        let err = match result {
            Ok(()) => {
                sent.extend(entries.iter().copied());
                log(&request, &entries, Outcome::Sent);
                continue;
            }
            Err(err) => err,
        };
        let failures = request.failures.saturating_add(1);
        if let Some(wait) = retry_wait(&err, failures, retry_delay) {
            log(&request, &entries, Outcome::Retrying(&err, wait));
            wants.retry(request, Instant::now() + wait);
            continue;
        }
        log(&request, &entries, Outcome::Dropped(&err));
        if matches!(err, RequestError::Unauthorized | RequestError::Closed) {
            wants.close();
            if matches!(err, RequestError::Unauthorized) {
                unauthorized();
            }
            return;
        }
    }
}

fn backoff(first: Duration, failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    first.saturating_mul(1 << doublings).min(RETRY_CAP)
}

// `None` when the ack isn't worth retrying.
fn retry_wait(err: &RequestError, failures: u32, first: Duration) -> Option<Duration> {
    let wait = backoff(first, failures);
    match err {
        RequestError::RateLimited { retry_after } => {
            Some(retry_after.map_or(wait, |after| wait.max(after.min(RETRY_CAP))))
        }
        err if err.is_transient() => Some(wait),
        _ => None,
    }
}

enum Outcome<'a> {
    Sent,
    Retrying(&'a RequestError, Duration),
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
        Outcome::Retrying(err, wait) => tracing::debug!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            pace,
            waited_ms,
            retry_ms = ms(wait),
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
