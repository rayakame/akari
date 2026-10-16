use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::mutes::until;
use crate::model::{ChannelId, MessageId};
use crate::ms;
use crate::rest::{AccountRest, Failure, RequestError, last_viewed};
use crate::state::{Ack, ReadState, wall_millis};

const TARGET: &str = "akari_core::acks";
// Like the official client: an ack about 2 s after the last message read.
const LULL: Duration = Duration::from_secs(2);
// A busy channel still reaches the user's other devices.
const CAP: Duration = Duration::from_secs(10);
const BULK_CHUNK: usize = 50;
// A failed ack's wait doubles up to this; a 429's retry_after counts at any length.
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
    // `alone`: from a refused bulk, so it goes in a request of its own.
    Bulk { at: Instant, alone: bool },
}

#[derive(Debug, Clone, Copy)]
struct Retry {
    at: Instant,
    // Failed requests in a row.
    failures: u32,
}

// The store's read state before the first of the merged acks, and the one the newest left.
#[derive(Debug, Clone, Copy)]
struct Before {
    state: Option<ReadState>,
    after: Option<ReadState>,
    // Taken before a READY that it hasn't been checked against yet.
    unchecked: bool,
    // Taken over the store a READY is replacing, so READY's state is what an undo goes back to.
    replaced: bool,
}

impl Before {
    fn now(state: Option<ReadState>, after: Option<ReadState>, replaced: bool) -> Self {
        Self {
            state,
            after,
            unchecked: false,
            replaced,
        }
    }

    // `self` came first, over Discord's position; a later ack was taken over Akari's own. One
    // still to be checked against READY doesn't hold back one taken since, the user's now.
    fn then(self, later: Self) -> Self {
        if self.unchecked && !later.unchecked {
            later
        } else {
            Self {
                state: self.state,
                after: later.after,
                unchecked: self.unchecked || later.unchecked,
                replaced: self.replaced || later.replaced,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Wanted {
    id: MessageId,
    before: Before,
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
            Kind::Explicit { at } | Kind::Bulk { at, .. } => at,
        };
        self.retry.map_or(due, |retry| due.max(retry.at))
    }

    fn bulk(&self) -> bool {
        matches!(self.kind, Kind::Bulk { .. })
    }

    fn alone(&self) -> bool {
        matches!(self.kind, Kind::Bulk { alone: true, .. })
    }

    fn pace(&self) -> Pace {
        match self.kind {
            Kind::Paced { .. } => Pace::Paced,
            Kind::Explicit { .. } | Kind::Bulk { .. } => Pace::Explicit,
        }
    }

    fn failures(&self) -> u32 {
        self.retry.map_or(0, |retry| retry.failures)
    }

    fn unchecked(&self) -> bool {
        self.before.unchecked
    }
}

#[derive(Debug)]
struct Request {
    entries: Vec<(ChannelId, MessageId)>,
    bulk: bool,
    pace: Pace,
    waited: Duration,
    failures: u32,
}

enum Next {
    Send(Request),
    At(Instant),
    Idle,
}

// The request on its way.
#[derive(Default)]
struct Flight {
    entries: Vec<(ChannelId, Wanted)>,
    // Discord may refuse a bulk for only some of its entries.
    several: bool,
}

// An ack Discord refused: the channel's read state before Akari's acks and the one the newest
// of them left, and the ID sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) channel: ChannelId,
    pub(crate) undo: Option<ReadState>,
    pub(crate) after: Option<ReadState>,
    pub(crate) sent: MessageId,
}

// At most one pending target per channel.
#[derive(Default)]
struct Queue {
    wanted: HashMap<ChannelId, Wanted>,
    flight: Flight,
    // Per channel, the highest ID Discord took since it last set the position on its own.
    sent: HashMap<ChannelId, MessageId>,
    // From `forget_all` to `reconcile`: the store is being replaced by READY's.
    swapping: bool,
    orders: u64,
    closed: bool,
}

impl Queue {
    fn order(&mut self) -> u64 {
        self.orders += 1;
        self.orders
    }

    fn want(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        before: Option<ReadState>,
        after: Option<ReadState>,
        pace: Pace,
        now: Instant,
    ) {
        if self.closed {
            return;
        }
        let order = self.order();
        let before = Before::now(before, after, self.swapping);
        match self.wanted.entry(channel) {
            Entry::Vacant(entry) => {
                let kind = match pace {
                    Pace::Paced => Kind::Paced { last: now },
                    Pace::Explicit => Kind::Explicit { at: now },
                };
                entry.insert(Wanted {
                    id,
                    before,
                    kind,
                    since: now,
                    order,
                    retry: None,
                });
            }
            Entry::Occupied(mut entry) => {
                let wanted = entry.get_mut();
                wanted.before = wanted.before.then(before);
                let advanced = id > wanted.id;
                wanted.id = wanted.id.max(id);
                wanted.kind = match (wanted.kind, pace) {
                    (Kind::Paced { .. }, Pace::Paced) if advanced => Kind::Paced { last: now },
                    (Kind::Paced { .. }, Pace::Explicit) => Kind::Explicit {
                        at: now.min(wanted.due()),
                    },
                    // The bulk's route may be what's limited; the channel's own goes ahead of it.
                    (Kind::Bulk { .. }, pace) if wanted.retry.is_some() => {
                        wanted.retry = None;
                        match pace {
                            Pace::Paced => Kind::Paced { last: now },
                            Pace::Explicit => Kind::Explicit { at: now },
                        }
                    }
                    (kind, _) => kind,
                };
            }
        }
    }

    fn want_bulk(&mut self, entries: Vec<BulkEntry>, now: Instant) {
        if self.closed {
            return;
        }
        for (channel, id, before, after) in entries {
            let order = self.order();
            let before = Before::now(before, after, self.swapping);
            match self.wanted.entry(channel) {
                Entry::Vacant(entry) => {
                    entry.insert(Wanted {
                        id,
                        before,
                        kind: Kind::Bulk {
                            at: now,
                            alone: false,
                        },
                        since: now,
                        order,
                        retry: None,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let wanted = entry.get_mut();
                    wanted.before = wanted.before.then(before);
                    wanted.id = wanted.id.max(id);
                    if !wanted.bulk() {
                        wanted.kind = Kind::Bulk {
                            at: now,
                            alone: false,
                        };
                        wanted.order = order;
                        wanted.retry = None;
                    }
                }
            }
        }
    }

    // Oldest due first; a request takes the bulk entries due in a row, up to a chunk. An entry
    // at or below what Discord took is dropped; one that READY may have overtaken waits.
    fn take(&mut self, now: Instant) -> Next {
        loop {
            let Some((&channel, &head)) = self
                .wanted
                .iter()
                .filter(|(_, wanted)| !wanted.unchecked())
                .min_by_key(|(_, wanted)| (wanted.due(), wanted.order))
            else {
                return Next::Idle;
            };
            if head.due() > now {
                return Next::At(head.due());
            }
            let channels = if head.bulk() && !head.alone() {
                let mut due: Vec<(Instant, u64, ChannelId, bool)> = self
                    .wanted
                    .iter()
                    .filter(|(_, wanted)| wanted.due() <= now && !wanted.unchecked())
                    .map(|(channel, wanted)| {
                        let joins = wanted.bulk() && !wanted.alone();
                        (wanted.due(), wanted.order, *channel, joins)
                    })
                    .collect();
                due.sort_unstable();
                due.into_iter()
                    .take_while(|(.., joins)| *joins)
                    .take(BULK_CHUNK)
                    .map(|(_, _, channel, _)| channel)
                    .collect()
            } else {
                vec![channel]
            };
            let mut request = Request {
                entries: Vec::with_capacity(channels.len()),
                bulk: head.bulk(),
                pace: head.pace(),
                waited: Duration::ZERO,
                failures: 0,
            };
            let mut flight = Vec::with_capacity(channels.len());
            for channel in channels {
                let Some(wanted) = self.wanted.remove(&channel) else {
                    continue;
                };
                if self
                    .sent
                    .get(&channel)
                    .is_some_and(|last| wanted.id <= *last)
                {
                    continue;
                }
                request.waited = request
                    .waited
                    .max(now.saturating_duration_since(wanted.since));
                request.failures = request.failures.max(wanted.failures());
                request.entries.push((channel, wanted.id));
                flight.push((channel, wanted));
            }
            if !request.entries.is_empty() {
                self.flight = Flight {
                    several: flight.len() > 1,
                    entries: flight,
                };
                return Next::Send(request);
            }
        }
    }

    // What READY may already have overtaken isn't remembered as Discord's.
    fn confirmed(&mut self) {
        for (channel, wanted) in std::mem::take(&mut self.flight).entries {
            if !wanted.unchecked() {
                self.sent.insert(channel, wanted.id);
            }
        }
    }

    // The request's targets wait until `at`, merged with what came for their channels since.
    fn retry(&mut self, at: Instant) {
        let flight = std::mem::take(&mut self.flight);
        if self.closed {
            return;
        }
        let failures = flight
            .entries
            .iter()
            .map(|(_, wanted)| wanted.failures())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        for (channel, mut wanted) in flight.entries {
            wanted.retry = Some(Retry { at, failures });
            self.requeue(channel, wanted);
        }
    }

    // The limiter holds the request's route until `at`: that's no failure, so no longer backoff.
    fn hold(&mut self, at: Instant) {
        let flight = std::mem::take(&mut self.flight);
        if self.closed {
            return;
        }
        for (channel, mut wanted) in flight.entries {
            wanted.retry = Some(Retry {
                at,
                failures: wanted.failures(),
            });
            self.requeue(channel, wanted);
        }
    }

    // A bulk of several entries goes again entry by entry. A single one is handed back, unless
    // a newer ack for the channel is pending: that one goes out and carries the undo.
    fn refuse(&mut self, now: Instant) -> Vec<Refusal> {
        let flight = std::mem::take(&mut self.flight);
        if self.closed {
            return Vec::new();
        }
        let mut refused = Vec::new();
        for (channel, mut wanted) in flight.entries {
            if flight.several {
                wanted.kind = Kind::Bulk {
                    at: now,
                    alone: true,
                };
                wanted.retry = None;
                self.requeue(channel, wanted);
            } else if let Some(newer) = self.wanted.get_mut(&channel) {
                newer.before = wanted.before.then(newer.before);
            } else {
                refused.push(Refusal {
                    channel,
                    undo: wanted.before.state,
                    after: wanted.before.after,
                    sent: wanted.id,
                });
            }
        }
        refused
    }

    fn requeue(&mut self, channel: ChannelId, back: Wanted) {
        let order = self.order();
        match self.wanted.entry(channel) {
            Entry::Vacant(entry) => {
                entry.insert(Wanted { order, ..back });
            }
            Entry::Occupied(mut entry) => {
                let newer = entry.get_mut();
                newer.id = newer.id.max(back.id);
                newer.before = back.before.then(newer.before);
                newer.since = newer.since.min(back.since);
                // A retry waits on its own route's limit, not on the other one's.
                if back.bulk() == newer.bulk() {
                    newer.retry = back.retry.or(newer.retry);
                }
                if let Kind::Bulk { alone, .. } = &mut newer.kind {
                    *alone |= back.alone();
                }
            }
        }
    }

    // Discord set the channel's position on its own: whatever Akari still had to send for it
    // could undo that.
    fn forget(&mut self, channel: ChannelId) {
        self.sent.remove(&channel);
        let queued = self.wanted.remove(&channel).is_some();
        let flying = self.flight.entries.len();
        self.flight.entries.retain(|(id, _)| *id != channel);
        if queued || self.flight.entries.len() < flying {
            tracing::debug!(
                target: TARGET,
                channel_id = channel.get(),
                "pending ack dropped: Discord's position was set elsewhere"
            );
        }
    }

    fn forget_all(&mut self) {
        self.sent.clear();
        self.swapping = true;
        for wanted in self.wanted.values_mut() {
            wanted.before.unchecked = true;
        }
        for (_, wanted) in &mut self.flight.entries {
            wanted.before.unchecked = true;
        }
    }

    // An ack taken before READY goes on only where READY has the position Akari acked over:
    // anything else is a read or a mark as unread elsewhere, which it mustn't undo.
    fn reconcile(&mut self, ready: impl Fn(ChannelId) -> Option<ReadState>) {
        self.swapping = false;
        let sent = &self.sent;
        let keep = |channel: &ChannelId, wanted: &mut Wanted| {
            let before = &mut wanted.before;
            let replaced = std::mem::take(&mut before.replaced);
            if before.unchecked {
                before.unchecked = false;
                if seen(ready(*channel)) != seen(before.state) {
                    tracing::debug!(
                        target: TARGET,
                        channel_id = channel.get(),
                        "pending ack dropped: Discord's position moved while Akari was away"
                    );
                    return false;
                }
                before.state = ready(*channel);
            } else if replaced && !sent.contains_key(channel) {
                // READY's state lacks an ack Discord took since; then the one taken over stays.
                before.state = ready(*channel);
            }
            true
        };
        self.wanted.retain(|channel, wanted| keep(channel, wanted));
        self.flight
            .entries
            .retain_mut(|(channel, wanted)| keep(channel, wanted));
    }

    fn close(&mut self) {
        self.closed = true;
        self.wanted = HashMap::new();
        self.flight = Flight::default();
    }
}

#[derive(Default)]
struct Wants {
    queue: Mutex<Queue>,
    changed: Notify,
    // See `AckSender::gate`.
    gate: Mutex<()>,
    #[cfg(test)]
    park: Mutex<Option<RefusalPark>>,
}

// Signals the refusal was taken, waits for the release, then signals it was handled.
#[cfg(test)]
type RefusalPark = (
    std::sync::mpsc::Sender<()>,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Sender<()>,
);

impl Wants {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn gate(&self) -> MutexGuard<'_, ()> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn want(
        &self,
        channel: ChannelId,
        id: MessageId,
        before: Option<ReadState>,
        after: Option<ReadState>,
        pace: Pace,
    ) {
        self.lock()
            .want(channel, id, before, after, pace, Instant::now());
        self.changed.notify_one();
    }

    fn want_bulk(&self, entries: Vec<BulkEntry>) {
        self.lock().want_bulk(entries, Instant::now());
        self.changed.notify_one();
    }

    fn confirmed(&self) {
        self.lock().confirmed();
    }

    fn retry(&self, at: Instant) {
        self.lock().retry(at);
    }

    fn hold(&self, at: Instant) {
        self.lock().hold(at);
    }

    fn refuse(&self) -> Vec<Refusal> {
        self.lock().refuse(Instant::now())
    }

    fn forget(&self, channel: ChannelId) {
        self.lock().forget(channel);
    }

    fn forget_all(&self) {
        self.lock().forget_all();
    }

    fn reconcile(&self, ready: impl Fn(ChannelId) -> Option<ReadState>) {
        self.lock().reconcile(ready);
        self.changed.notify_one();
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

type Unauthorized = Box<dyn Fn() + Send + Sync>;
type Refused = Box<dyn Fn(Refusal) + Send + Sync>;
// The channel, the ID, and the store's read state before and after the local ack.
type BulkEntry = (ChannelId, MessageId, Option<ReadState>, Option<ReadState>);

// Closing or dropping it drops what's pending; a request already on its way isn't cancelled.
pub(crate) struct AckSender {
    wants: Arc<Wants>,
    rest: Arc<AccountRest>,
    #[cfg(test)]
    handed: Mutex<Vec<Handed>>,
}

impl AckSender {
    // `unauthorized` runs once on a 401, which stops the sender; `refused` gets each ack Discord
    // won't take. A failed ack waits `retry_delay`, doubling to a minute, or a longer retry_after.
    pub(crate) fn start(
        runtime: &tokio::runtime::Handle,
        rest: Arc<AccountRest>,
        retry_delay: Duration,
        unauthorized: impl Fn() + Send + Sync + 'static,
        refused: impl Fn(Refusal) + Send + Sync + 'static,
    ) -> Self {
        let wants = Arc::new(Wants::default());
        runtime.spawn(send(
            wants.clone(),
            rest.clone(),
            retry_delay,
            Box::new(unauthorized),
            Box::new(refused),
        ));
        Self {
            wants,
            rest,
            #[cfg(test)]
            handed: Mutex::default(),
        }
    }

    // Returns at once; a pending ack for the channel merges with this one.
    pub(crate) fn want(&self, ack: Ack, pace: Pace) {
        #[cfg(test)]
        self.hand(Handed::One(ack.channel, ack.into(), pace));
        if let Some(id) = self.resolve(ack) {
            self.wants
                .want(ack.channel, id, ack.before, ack.after, pace);
        }
    }

    // Explicit; takes over the channels' pending single acks.
    pub(crate) fn want_bulk(&self, acks: Vec<Ack>) {
        #[cfg(test)]
        self.hand(Handed::Bulk(
            acks.iter()
                .map(|ack| (ack.channel, (*ack).into()))
                .collect(),
        ));
        let entries = acks
            .into_iter()
            .filter_map(|ack| Some((ack.channel, self.resolve(ack)?, ack.before, ack.after)))
            .collect();
        self.wants.want_bulk(entries);
    }

    // "Now" is when the user acted, not when the ack goes out.
    fn resolve(&self, ack: Ack) -> Option<MessageId> {
        let id = AckTarget::from(ack).resolve(self.rest.server_now_millis());
        if id.is_none() {
            tracing::debug!(
                target: TARGET,
                channel_id = ack.channel.get(),
                "no ack: neither Discord's time nor a message is known"
            );
        }
        id
    }

    // Discord set the channel's position on its own, maybe behind what was sent (a mark as unread
    // elsewhere): what's pending for it is dropped; the next ack goes out even if it isn't newer.
    pub(crate) fn forget(&self, channel: ChannelId) {
        self.wants.forget(channel);
    }

    // A READY is coming: its read states are Discord's, whatever was sent before. What's pending
    // waits for `reconcile`.
    pub(crate) fn forget_all(&self) {
        self.wants.forget_all();
    }

    // With READY's read states as it delivered them: drops what was pending at `forget_all` where
    // Discord moved. Refused, the rest goes back to READY's state.
    pub(crate) fn reconcile(&self, ready: impl Fn(ChannelId) -> Option<ReadState>) {
        self.wants.reconcile(ready);
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

    // Taken before the store or the queue, around a local ack and its `want`, `forget_all` and
    // READY's journal, READY's swap and `reconcile`, a refusal and its undo. Never across an await.
    pub(crate) fn gate(&self) -> MutexGuard<'_, ()> {
        self.wants.gate()
    }

    // The next refusal waits after it's taken from the queue until the test releases it. Blocks
    // a runtime worker, so only for multi-threaded tests.
    #[cfg(test)]
    pub(crate) fn park_next_refusal(
        &self,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
        std::sync::mpsc::Receiver<()>,
    ) {
        let (taken, on_taken) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        let (handled, on_handled) = std::sync::mpsc::channel();
        *self
            .wants
            .park
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((taken, on_release, handled));
        (on_taken, release, on_handled)
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
    unauthorized: Unauthorized,
    refused: Refused,
) {
    while let Some(request) = wants.next().await {
        let last_viewed = last_viewed(rest.server_now_millis().unwrap_or_else(wall_millis));
        let result = match request.entries[..] {
            [(channel, id)] if !request.bulk => rest.ack(channel, id, last_viewed).await,
            _ => rest.ack_bulk(&request.entries).await,
        };
        let err = match result {
            Ok(()) => {
                wants.confirmed();
                log(&request, Outcome::Sent);
                continue;
            }
            Err(Failure::Held(wait)) => match Instant::now().checked_add(wait) {
                Some(at) => {
                    log(&request, Outcome::Held(wait));
                    wants.hold(at);
                    continue;
                }
                None => RequestError::RateLimited {
                    retry_after: Some(wait),
                },
            },
            Err(Failure::Request(err)) => err,
        };
        if matches!(err, RequestError::Unauthorized | RequestError::Closed) {
            log(&request, Outcome::Dropped(&err));
            wants.close();
            if matches!(err, RequestError::Unauthorized) {
                unauthorized();
            }
            return;
        }
        let failures = request.failures.saturating_add(1);
        if let Some(wait) = retry_wait(&err, failures, retry_delay)
            && let Some(at) = Instant::now().checked_add(wait)
        {
            log(&request, Outcome::Retrying(&err, wait));
            wants.retry(at);
            continue;
        }
        log(&request, Outcome::Refused(&err));
        let _gate = wants.gate();
        let refusals = wants.refuse();
        #[cfg(test)]
        let park = wants
            .park
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        #[cfg(test)]
        if let Some((taken, release, _)) = &park {
            let _ = taken.send(());
            let _ = release.recv();
        }
        for refusal in refusals {
            refused(refusal);
        }
        #[cfg(test)]
        if let Some((.., handled)) = park {
            let _ = handled.send(());
        }
    }
}

// Where Discord had the channel as far as Akari knew; an estimated position was Akari's.
fn seen(state: Option<ReadState>) -> Option<MessageId> {
    state
        .filter(|state| !state.estimated)
        .and_then(|state| state.acked)
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
            Some(retry_after.map_or(wait, |after| wait.max(after)))
        }
        err if err.is_transient() => Some(wait),
        _ => None,
    }
}

enum Outcome<'a> {
    Sent,
    Held(Duration),
    Retrying(&'a RequestError, Duration),
    Refused(&'a RequestError),
    Dropped(&'a RequestError),
}

fn log(request: &Request, outcome: Outcome<'_>) {
    let (channel_id, message_id) = match request.entries[..] {
        [(channel, id)] => (Some(channel.get()), Some(id.get())),
        _ => (None, None),
    };
    let entries = request.bulk.then_some(request.entries.len());
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
        Outcome::Held(wait) => tracing::debug!(
            target: TARGET,
            channel_id,
            message_id,
            entries,
            pace,
            waited_ms,
            retry_ms = ms(wait),
            "ack held by the rate limiter"
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
        Outcome::Refused(err) => {
            let (status, code) = match err {
                RequestError::Discord { status, code, .. } => (Some(*status), Some(*code)),
                _ => (None, None),
            };
            if request.entries.len() > 1 {
                tracing::warn!(
                    target: TARGET,
                    entries,
                    error = err.kind(),
                    status,
                    code,
                    "ack bulk refused; sending its entries one by one"
                );
            } else {
                tracing::warn!(
                    target: TARGET,
                    channel_id,
                    message_id,
                    error = err.kind(),
                    status,
                    code,
                    "ack refused"
                );
            }
        }
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
