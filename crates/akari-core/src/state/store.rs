use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::{Mutex, Notify, mpsc};

use super::apply::{Derived, Entities, ReadyDiff, State};
use super::badges::{Badge, Badges};
use super::clock::Clock;
use super::events::{ConnectionState, StoreEvent};
use super::order;
use super::read_states::{Ack, ReadState};
use super::send::Slowmode;
use super::settings::{GuildSettings, NotificationLevel, ServerListItem};
use super::types::{Channel, CurrentUser, Guild, Member, Message, StarterMessage, User};
#[cfg(test)]
use super::windows::DEFAULT_LIMITS;
use super::windows::{
    CachedWindow, LoadKind, LoadTicket, MessageWindow, StarterFetch, WindowLimits,
};
use crate::backlog::Backlog;
use crate::gateway::{DispatchEvent, GatewayGuild, Ready};
use crate::load_report::PageOutcome;
use crate::model::{self, ChannelId, GuildId, MessageId, Permissions, Timestamp, UserId};

#[cfg(test)]
thread_local! {
    static DEADLINE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How often the deadline task read its next deadline on this thread.
#[cfg(test)]
pub(crate) fn deadline_reads() -> usize {
    DEADLINE_READS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn deadline_reads_reset() {
    DEADLINE_READS.with(|reads| reads.set(0));
}

// Main-thread readers (SwiftUI, AppKit) wait at most this long for a write.
const LONG_HOLD: Duration = Duration::from_millis(4);

pub(crate) struct PreparedReady {
    entities: Entities,
    derived: Derived,
    diff: Option<ReadyDiff>,
    read_states: HashMap<ChannelId, ReadState>,
    preparing: Preparing,
}

// What a READY's swap leaves: its read states as it delivered them, before the local acks replayed
// on top, and the state it replaced, for the caller to drop outside its locks.
pub(crate) struct Replaced {
    pub(crate) read_states: HashMap<ChannelId, ReadState>,
    _old: (Entities, Derived),
}

// An open READY journal. Ends it if dropped before the swap, e.g. by a panic while converting.
pub(crate) struct Preparing {
    store: Store,
    swapped: bool,
}

impl Drop for Preparing {
    fn drop(&mut self) {
        if !self.swapped {
            self.store
                .write("READY", |inner, _| inner.state.end_journal());
        }
    }
}

/// An account's state, kept current by its gateway connection. Cheap to clone. Reads
/// return snapshots: values later changes never touch.
#[derive(Clone)]
pub struct Store {
    shared: Arc<Shared>,
}

struct Shared {
    inner: RwLock<Inner>,
    // What the badges and the deadlines call now.
    clock: Arc<Clock>,
    // A write brought a deadline sooner than the ones the deadline task knew.
    sooner: Notify,
    // Bumped by every write, so the disk cache skips snapshots while nothing changed.
    changes: AtomicU64,
    // Hands out the order of loads and supersedes that place a window, without the lock.
    placings: AtomicU64,
    #[cfg(test)]
    park: std::sync::Mutex<Option<Park>>,
    #[cfg(test)]
    ready_thread: std::sync::Mutex<Option<std::thread::ThreadId>>,
    #[cfg(test)]
    slow_write: std::sync::Mutex<Option<std::time::Duration>>,
    #[cfg(test)]
    long_holds: std::sync::Mutex<Vec<(&'static str, std::time::Duration)>>,
    #[cfg(test)]
    holds: std::sync::Mutex<Option<Vec<(&'static str, std::time::Duration)>>>,
}

#[cfg(test)]
type Park = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);

struct Inner {
    state: State,
    connection: ConnectionState,
    subscribers: Vec<Subscriber>,
    finished: bool,
}

struct Subscriber {
    events: mpsc::UnboundedSender<StoreEvent>,
    buffered: Arc<AtomicUsize>,
    backlog: Backlog,
}

/// The change events of a [`Store`]. Unread events are buffered; the store never waits
/// for a subscriber.
pub struct Subscription {
    events: Mutex<mpsc::UnboundedReceiver<StoreEvent>>,
    buffered: Arc<AtomicUsize>,
}

impl Subscription {
    /// The next change. `None` once the account is closed and every event has been read.
    pub async fn next(&self) -> Option<StoreEvent> {
        let event = self.events.lock().await.recv().await?;
        self.buffered.fetch_sub(1, Ordering::Relaxed);
        Some(event)
    }

    /// The next changes in order, at most `max` and at least one; waits while none are
    /// buffered. Empty once the account is closed and every event has been read.
    pub async fn next_batch(&self, max: usize) -> Vec<StoreEvent> {
        let mut batch = Vec::new();
        let taken = self
            .events
            .lock()
            .await
            .recv_many(&mut batch, max.max(1))
            .await;
        self.buffered.fetch_sub(taken, Ordering::Relaxed);
        batch
    }

    pub(crate) fn buffered(&self) -> usize {
        self.buffered.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Inner {
    // Runs while the write lock is held, so every subscriber sees changes in their order.
    fn publish(&mut self, events: Vec<StoreEvent>) {
        if events.is_empty() {
            return;
        }
        self.subscribers.retain_mut(|subscriber| {
            for event in &events {
                let buffered = subscriber.buffered.fetch_add(1, Ordering::Relaxed) + 1;
                if subscriber.backlog.warns_at(buffered) {
                    tracing::warn!(buffered, "store events are piling up unread");
                }
                if subscriber.events.send(event.clone()).is_err() {
                    return false;
                }
            }
            true
        });
    }
}

fn same(a: &ConnectionState, b: &ConnectionState) -> bool {
    matches!(
        (a, b),
        (ConnectionState::Offline, ConnectionState::Offline)
            | (ConnectionState::Connecting, ConnectionState::Connecting)
            | (ConnectionState::Online, ConnectionState::Online)
    )
}

impl Store {
    pub(crate) fn new(limits: WindowLimits) -> Self {
        let clock = Arc::new(Clock::new());
        Self {
            shared: Arc::new(Shared {
                changes: AtomicU64::new(0),
                placings: AtomicU64::new(0),
                clock: clock.clone(),
                sooner: Notify::new(),
                inner: RwLock::new(Inner {
                    state: State::with_limits(limits, clock),
                    connection: ConnectionState::Offline,
                    subscribers: Vec::new(),
                    finished: false,
                }),
                #[cfg(test)]
                park: std::sync::Mutex::new(None),
                #[cfg(test)]
                ready_thread: std::sync::Mutex::new(None),
                #[cfg(test)]
                slow_write: std::sync::Mutex::new(None),
                #[cfg(test)]
                long_holds: std::sync::Mutex::new(Vec::new()),
                #[cfg(test)]
                holds: std::sync::Mutex::new(None),
            }),
        }
    }

    // Never steps back, so a window or mute that ended stays ended.
    pub(crate) fn now(&self) -> Timestamp {
        Timestamp::from_unix_millis(self.now_millis())
    }

    fn now_millis(&self) -> i64 {
        self.shared.clock.now_millis()
    }

    fn read<T>(&self, read: impl FnOnce(&Inner) -> T) -> T {
        let inner = self
            .shared
            .inner
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        #[cfg(test)]
        super::badges::hooks::reading(true);
        let value = read(&inner);
        #[cfg(test)]
        super::badges::hooks::reading(false);
        value
    }

    fn write<T>(
        &self,
        event: &'static str,
        write: impl FnOnce(&mut Inner, &mut Vec<StoreEvent>) -> T,
    ) -> T {
        let mut inner = self
            .shared
            .inner
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let taken = Instant::now();
        let mut events = Vec::new();
        #[cfg(test)]
        super::badges::hooks::writing(true);
        let value = write(&mut inner, &mut events);
        let sooner = inner.state.take_sooner_deadline();
        #[cfg(test)]
        super::badges::hooks::writing(false);
        self.shared.changes.fetch_add(1, Ordering::Relaxed);
        inner.publish(events);
        #[cfg(test)]
        self.slow_down();
        let held = taken.elapsed();
        drop(inner);
        if sooner {
            self.shared.sooner.notify_one();
        }
        #[cfg(test)]
        if let Ok(mut holds) = self.shared.holds.lock()
            && let Some(holds) = holds.as_mut()
        {
            holds.push((event, held));
        }
        if held > LONG_HOLD {
            tracing::warn!(
                event,
                held_ms = held.as_millis(),
                "the store's write lock was held too long"
            );
            #[cfg(test)]
            if let Ok(mut long) = self.shared.long_holds.lock() {
                long.push((event, held));
            }
        }
        value
    }

    // Returns the ack of the channel being read, when the event left it unread.
    pub(crate) fn apply(&self, event: DispatchEvent) -> Option<Ack> {
        match event {
            DispatchEvent::Ready(ready) => {
                let next = self.prepare_ready(*ready);
                self.replace(next);
                None
            }
            DispatchEvent::GuildCreate(guild) => match *guild {
                GatewayGuild::Available(guild) => {
                    // A large guild takes a while to convert; readers mustn't wait for it.
                    let me = self.read(|inner| inner.state.me());
                    let next = Entities::from_guild(*guild, me);
                    #[cfg(test)]
                    self.park();
                    self.write("GUILD_CREATE", |inner, events| {
                        inner.state.add_guild(next, events);
                        inner.state.take_reading_ack()
                    })
                }
                unavailable => {
                    let event = DispatchEvent::GuildCreate(Box::new(unavailable));
                    self.write("GUILD_CREATE", |inner, events| {
                        inner.state.apply(event, events);
                        inner.state.take_reading_ack()
                    })
                }
            },
            DispatchEvent::NotificationSettingsUpdate(settings) => {
                self.set_notification_flags(settings.flags);
                None
            }
            event => {
                let name = event.name();
                self.write(name, |inner, events| {
                    inner.state.apply(event, events);
                    inner.state.take_reading_ack()
                })
            }
        }
    }

    // New flags can move every badge. They are built from a copy before the write lock; a write
    // in between means the copy is stale, and they are rebuilt under the lock instead.
    fn set_notification_flags(&self, flags: u64) {
        let start = self.shared.changes.load(Ordering::SeqCst);
        let Some(next) = self.read(|inner| inner.state.with_notification_flags(flags)) else {
            return;
        };
        let badges = Badges::build(&next, self.now_millis());
        drop(next);
        self.write("NOTIFICATION_SETTINGS_UPDATE", |inner, events| {
            let current = self.shared.changes.load(Ordering::SeqCst) == start;
            inner
                .state
                .set_notification_flags(flags, current.then_some(badges), events);
        });
    }

    pub(crate) fn prepare_ready(&self, ready: Ready) -> PreparedReady {
        self.prepare(self.begin_ready(), ready)
    }

    // From here until the swap, local acks also go into READY's journal.
    pub(crate) fn begin_ready(&self) -> Preparing {
        self.write("READY", |inner, _| inner.state.begin_journal());
        Preparing {
            store: self.clone(),
            swapped: false,
        }
    }

    // Converting and diffing a large READY takes milliseconds, so readers mustn't wait for
    // it. Only the pump changes entities, apart from local acks, which the swap replays.
    pub(crate) fn prepare(&self, preparing: Preparing, ready: Ready) -> PreparedReady {
        let mut entities = Entities::from_ready(ready);
        let read_states = entities.read_states.channels().clone();
        self.read(|inner| inner.state.keep_memberships(&mut entities));
        let derived = Derived::build(&entities, self.now_millis());
        let diff = self.read(|inner| {
            inner.state.keep_folders(&mut entities);
            inner.state.ready_diff(&entities, &derived)
        });
        #[cfg(test)]
        {
            if let Ok(mut thread) = self.shared.ready_thread.lock() {
                *thread = Some(std::thread::current().id());
            }
            self.park();
        }
        PreparedReady {
            entities,
            derived,
            diff,
            read_states,
            preparing,
        }
    }

    pub(crate) fn replace(&self, next: PreparedReady) -> Replaced {
        let PreparedReady {
            entities,
            derived,
            diff,
            read_states,
            mut preparing,
        } = next;
        let old = self.write("READY", |inner, events| {
            inner.state.swap(entities, derived, diff, events)
        });
        preparing.swapped = true;
        Replaced {
            read_states,
            _old: old,
        }
    }

    pub(crate) fn set_connection(&self, connection: ConnectionState) {
        self.set_connection_if(connection, || true);
    }

    // `allowed` runs under the write lock, so a change made before it can't be overtaken.
    pub(crate) fn set_connection_if(
        &self,
        connection: ConnectionState,
        allowed: impl FnOnce() -> bool,
    ) {
        self.write("connection", |inner, events| {
            let closed = matches!(inner.connection, ConnectionState::Closed { .. });
            if !closed && !same(&inner.connection, &connection) && allowed() {
                inner.connection = connection.clone();
                events.push(StoreEvent::Connection(connection));
            }
        });
    }

    pub(crate) fn begin_connecting(&self) {
        self.write("connection", |inner, events| {
            if matches!(inner.connection, ConnectionState::Offline) {
                inner.connection = ConnectionState::Connecting;
                events.push(StoreEvent::Connection(ConnectionState::Connecting));
            }
        });
    }

    pub(crate) fn next_placing(&self) -> u64 {
        self.shared.placings.fetch_add(1, Ordering::Relaxed) + 1
    }

    // `placing` from `next_placing` where the host asked for the load; `None` takes one now.
    pub(crate) fn begin_load(
        &self,
        channel: ChannelId,
        kind: LoadKind,
        placing: Option<u64>,
    ) -> Option<LoadTicket> {
        let placing = placing.unwrap_or_else(|| self.next_placing());
        let mut ticket = None;
        self.write("load", |inner, events| {
            ticket = inner.state.begin_load(channel, kind, placing, events);
        });
        ticket
    }

    pub(crate) fn finish_load(
        &self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        reached_end: bool,
    ) -> (PageOutcome, Option<Ack>) {
        self.write("load", |inner, events| {
            let outcome = inner.state.finish_load(ticket, page, reached_end, events);
            (outcome, inner.state.take_reading_ack())
        })
    }

    // The parent message to fetch for `thread`'s starter, if it's needed and not asked for yet.
    // Read first: a write for every page would bump `changes` and the cache's snapshots.
    pub(crate) fn starter_to_fetch(&self, thread: ChannelId) -> Option<(ChannelId, MessageId)> {
        if !self.read(|inner| inner.state.wants_starter(thread)) {
            return None;
        }
        self.write("starter", |inner, _| inner.state.starter_to_fetch(thread))
    }

    pub(crate) fn settle_starter(&self, thread: ChannelId, fetch: StarterFetch) {
        self.write("starter", |inner, events| {
            inner.state.settle_starter(thread, fetch, events);
        });
    }

    pub(crate) fn learn_thread(&self, thread: model::Channel) -> bool {
        self.write("thread", |inner, events| {
            inner.state.learn_thread(thread, events)
        })
    }

    pub(crate) fn mark_thread_unavailable(&self, parent: ChannelId, thread: ChannelId) {
        self.write("thread", |inner, events| {
            inner.state.mark_thread_unavailable(parent, thread, events);
        });
    }

    pub(crate) fn fail_starter(&self, thread: ChannelId) {
        self.write("starter", |inner, events| {
            inner.state.fail_starter(thread, events);
        });
    }

    pub(crate) fn supersede_around(&self, channel: ChannelId) {
        let placing = self.next_placing();
        self.write("load", |inner, _| {
            inner.state.supersede_around(channel, placing);
        });
    }

    // Returns the channel's ack when its list shows unread messages; before the first READY, none.
    pub(crate) fn set_reading(&self, channel: Option<ChannelId>) -> Option<Ack> {
        self.write("reading", |inner, events| {
            inner.state.set_reading(channel, events)
        })
    }

    // A connection that dropped may come back in a new session: until the host says the user
    // still reads the channel, nothing acks it.
    pub(crate) fn stop_reading(&self) {
        self.write("reading", |inner, events| {
            inner.state.set_reading(None, events);
        });
    }

    pub(crate) fn undo_ack(
        &self,
        channel: ChannelId,
        undo: Option<ReadState>,
        after: Option<ReadState>,
    ) -> bool {
        self.write("undo", |inner, events| {
            inner.state.undo_ack(channel, undo, after, events)
        })
    }

    // `None` when the channel was read already.
    pub(crate) fn mark_read(&self, channel: ChannelId) -> Option<Ack> {
        self.write("mark read", |inner, events| {
            inner.state.mark_read(channel, events)
        })
    }

    // Empty when nothing but forums would be acked.
    pub(crate) fn mark_guilds_read(&self, guilds: &[GuildId]) -> Vec<Ack> {
        let now = self.now_millis();
        self.write("mark read", |inner, events| {
            inner.state.mark_guilds_read(guilds, now, events)
        })
    }

    pub(crate) fn restore(&self, cached: Entities) -> bool {
        let derived = Derived::build(&cached, self.now_millis());
        let restored = self.write("restore", |inner, events| {
            inner.state.restore(cached, derived, events)
        });
        if !restored {
            tracing::info!(target: "akari_core::cache", reason = "ready_first", "cache not used");
        }
        restored
    }

    pub(crate) fn fill_from_cache(&self, ticket: LoadTicket, messages: Vec<Arc<Message>>) {
        self.write("load", |inner, events| {
            inner.state.fill_from_cache(ticket, messages, events);
        });
    }

    pub(crate) fn abort_load(&self, ticket: LoadTicket) {
        self.write("load", |inner, events| {
            inner.state.abort_load(ticket, events)
        });
    }

    // Waits until a write brings a deadline before the ones ahead; a write before the call counts.
    pub(crate) async fn deadline_sooner(&self) {
        self.shared.sooner.notified().await;
    }

    // The next mute end or thread window end after `after`.
    pub(crate) fn next_deadline(&self, after: Timestamp) -> Option<Timestamp> {
        #[cfg(test)]
        DEADLINE_READS.with(|reads| reads.set(reads.get() + 1));
        self.read(|inner| {
            let mutes = inner.state.next_mute_end(after);
            mutes
                .into_iter()
                .chain(inner.state.next_window_end(after))
                .min()
        })
    }

    // Reports the guilds whose mute, or a channel's, ended within `after..=until`, and the
    // threads whose window did.
    pub(crate) fn end_deadlines(&self, after: Timestamp, until: Timestamp) {
        let ended = self.read(|inner| inner.state.mutes_ending(after, until));
        if !ended.is_empty() {
            self.write("mutes", |inner, events| {
                inner.state.end_mutes(&ended, until.unix_millis(), events);
            });
        }
        let ended = self.read(|inner| inner.state.windows_ending(after, until));
        if !ended.is_empty() {
            self.write("windows", |inner, events| {
                inner.state.end_windows(&ended, until.unix_millis(), events);
            });
        }
    }

    pub(crate) fn changes(&self) -> u64 {
        self.shared.changes.load(Ordering::Relaxed)
    }

    // Arc clones under the read lock; the disk work happens elsewhere.
    pub(crate) fn cache_parts(
        &self,
        per_window: usize,
    ) -> (Entities, Vec<(ChannelId, CachedWindow)>) {
        self.read(|inner| inner.state.cache_parts(per_window))
    }

    pub(crate) fn had_ready(&self) -> bool {
        self.read(|inner| inner.state.had_ready())
    }

    pub(crate) fn stale_channels(&self) -> Vec<ChannelId> {
        self.read(|inner| inner.state.stale_channels())
    }

    pub(crate) fn viewed_channels(&self) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.viewed_channels())
    }

    pub(crate) fn queue_message(&self, channel: ChannelId, message: Arc<Message>) {
        let now = self.now_millis();
        self.write("send", |inner, events| {
            inner.state.queue(channel, message, now, events)
        });
    }

    pub(crate) fn confirm_message(
        &self,
        channel: ChannelId,
        pending: MessageId,
        message: model::Message,
    ) {
        self.write("send", |inner, events| {
            inner.state.confirm(channel, pending, message, events);
        });
    }

    pub(crate) fn fail_message(&self, channel: ChannelId, pending: MessageId) {
        self.write("send", |inner, events| {
            inner.state.fail(channel, pending, events)
        });
    }

    pub(crate) fn retry_message(
        &self,
        channel: ChannelId,
        pending: MessageId,
    ) -> Option<Arc<Message>> {
        let mut message = None;
        let now = self.now_millis();
        self.write("send", |inner, events| {
            message = inner.state.retry(channel, pending, now, events);
        });
        message
    }

    pub(crate) fn pending_message(
        &self,
        channel: ChannelId,
        id: MessageId,
    ) -> Option<Arc<Message>> {
        self.read(|inner| inner.state.pending(channel, id))
    }

    pub(crate) fn drop_cooldown(&self, channel: ChannelId, send: MessageId) {
        self.write("send", |inner, _| inner.state.drop_cooldown(channel, send));
    }

    pub(crate) fn hold_cooldown(&self, channel: ChannelId, wait: Duration) {
        let now = self.now_millis();
        self.write("send", |inner, _| {
            inner.state.hold_cooldown(channel, wait, now);
        });
    }

    pub(crate) fn discard_message(&self, channel: ChannelId, pending: MessageId) {
        self.write("send", |inner, events| {
            inner.state.discard(channel, pending, events)
        });
    }

    pub(crate) fn view_channel(&self, channel: ChannelId) {
        self.write("view", |inner, events| {
            inner.state.view_channel(channel, events)
        });
    }

    pub(crate) fn finish(&self) {
        self.write("finish", |inner, _| {
            inner.finished = true;
            inner.subscribers.clear();
        });
    }

    /// Changes from now on, in the order they happen. Subscribe first, then read, then
    /// apply the events: one may describe a change the read already shows, and applying
    /// it again is harmless.
    pub fn subscribe(&self) -> Subscription {
        let (sender, receiver) = mpsc::unbounded_channel();
        let buffered = Arc::new(AtomicUsize::new(0));
        self.write("subscribe", |inner, _| {
            if !inner.finished {
                inner.subscribers.push(Subscriber {
                    events: sender,
                    buffered: buffered.clone(),
                    backlog: Backlog::default(),
                });
            }
        });
        Subscription {
            events: Mutex::new(receiver),
            buffered,
        }
    }

    pub fn connection(&self) -> ConnectionState {
        self.read(|inner| inner.connection.clone())
    }

    /// `None` before the first READY.
    pub fn current_user(&self) -> Option<Arc<CurrentUser>> {
        self.read(|inner| inner.state.current_user())
    }

    /// A DM or group DM recipient.
    pub fn user(&self, id: UserId) -> Option<Arc<User>> {
        self.read(|inner| inner.state.user(id))
    }

    /// Available guilds, in no particular order.
    pub fn guilds(&self) -> Vec<Arc<Guild>> {
        self.read(|inner| inner.state.guilds())
    }

    /// The server list in the user's order, with their folders. Guilds the settings don't
    /// place yet (joined since) come first, the newest join first. Unavailable guilds are
    /// left out.
    pub fn server_list(&self) -> Vec<ServerListItem> {
        self.read(|inner| inner.state.server_list())
    }

    /// Available guilds in server list order: [`server_list`](Self::server_list) with every
    /// folder's guilds in its place.
    pub fn guild_list(&self) -> Vec<Arc<Guild>> {
        self.read(|inner| {
            let state = &inner.state;
            state
                .server_list()
                .into_iter()
                .flat_map(|item| match item {
                    ServerListItem::Guild(id) => vec![id],
                    ServerListItem::Folder(folder) => folder.guilds.to_vec(),
                })
                .filter_map(|id| state.guild(id))
                .collect()
        })
    }

    /// The user's notification settings for a guild, or for DMs with `None`.
    pub fn guild_settings(&self, guild: Option<GuildId>) -> Arc<GuildSettings> {
        self.read(|inner| inner.state.guild_settings(guild))
    }

    /// Muted now by its own settings, for a thread by its own membership. Neither a muted
    /// category nor a muted guild makes its channels muted, nor a muted channel its threads;
    /// see [`guild_settings`](Self::guild_settings).
    pub fn channel_muted(&self, channel: ChannelId) -> bool {
        let now = self.now();
        self.read(|inner| inner.state.channel_muted_at(channel, now))
    }

    /// Of `ids`, the channels muted now by their own settings, in the order given.
    pub fn muted_channels(&self, ids: &[ChannelId]) -> Vec<ChannelId> {
        let now = self.now();
        self.read(|inner| {
            ids.iter()
                .copied()
                .filter(|id| inner.state.channel_muted_at(*id, now))
                .collect()
        })
    }

    /// Whether the guild (`None`: DMs) is muted now. Its channels aren't muted by it.
    pub fn guild_muted(&self, guild: Option<GuildId>) -> bool {
        let now = self.now();
        self.read(|inner| inner.state.guild_settings(guild).muted_now(now))
    }

    /// The level that decides which messages notify in a channel: its own, else its
    /// category's, else the guild's, else the guild's default.
    pub fn notification_level(&self, channel: ChannelId) -> NotificationLevel {
        self.read(|inner| inner.state.notification_level(channel))
    }

    pub fn guild(&self, id: GuildId) -> Option<Arc<Guild>> {
        self.read(|inner| inner.state.guild(id))
    }

    /// Guilds that are down or blocked in the user's region.
    pub fn unavailable_guilds(&self) -> Vec<GuildId> {
        self.read(|inner| inner.state.unavailable_guilds())
    }

    /// The current user's membership in a guild.
    pub fn current_member(&self, guild: GuildId) -> Option<Arc<Member>> {
        self.read(|inner| inner.state.current_member(guild))
    }

    /// Whether `message` mentions the current user: by ID, through @everyone or @here, or
    /// through one of the current member's roles. Suppress settings don't change it: the
    /// official client still highlights such a message and only leaves out its badge.
    pub fn mentions_me(&self, message: &Message) -> bool {
        self.read(|inner| {
            let state = &inner.state;
            let Some(me) = state.current_user() else {
                return false;
            };
            if message.mention_everyone || message.mentions.iter().any(|user| user.id == me.user.id)
            {
                return true;
            }
            !message.mention_roles.is_empty()
                && state
                    .channel(message.channel_id)
                    .and_then(|channel| channel.guild_id)
                    .and_then(|guild| state.current_member(guild))
                    .is_some_and(|member| {
                        member
                            .roles
                            .iter()
                            .any(|role| message.mention_roles.contains(role))
                    })
        })
    }

    /// What the channel's row shows. `Badge::default()` for channels the user can't view
    /// and for kinds without messages, such as categories.
    pub fn channel_badge(&self, channel: ChannelId) -> Badge {
        self.read(|inner| inner.state.channel_badge(channel))
    }

    /// The badges of `ids`, in the order given, in one read.
    pub fn channel_badges(&self, ids: &[ChannelId]) -> Vec<Badge> {
        self.read(|inner| {
            ids.iter()
                .map(|id| inner.state.channel_badge(*id))
                .collect()
        })
    }

    /// The badges of `ids`, in the order given, in one read.
    pub fn guild_badges(&self, ids: &[GuildId]) -> Vec<Badge> {
        self.read(|inner| ids.iter().map(|id| inner.state.guild_badge(*id)).collect())
    }

    /// The sum over the guild's channels and listed threads that count for it. `unread` is
    /// false while the guild is muted; its mentions stay.
    pub fn guild_badge(&self, guild: GuildId) -> Badge {
        self.read(|inner| inner.state.guild_badge(guild))
    }

    /// DMs and group DMs with unread mentions, the newest message first: the rail's DM
    /// icons. Every message in an unmuted DM counts as a mention.
    pub fn unread_private_channels(&self) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.unread_private_channels())
    }

    /// Where the user stopped reading the channel, and its unread mentions. `None` while there
    /// is none: Discord sent none and no message or ack has come since, so a guild channel
    /// counts as read up to when the user joined and a DM until its next message. One the
    /// store started itself is `estimated`.
    pub fn read_state(&self, channel: ChannelId) -> Option<ReadState> {
        self.read(|inner| inner.state.read_state(channel))
    }

    /// A guild channel, category, thread, DM or group DM.
    pub fn channel(&self, id: ChannelId) -> Option<Arc<Channel>> {
        self.read(|inner| inner.state.channel(id))
    }

    /// Whether [`Account::fetch_thread`](crate::Account::fetch_thread) found this session that
    /// the user can't see the thread, or that it is gone.
    pub fn thread_unavailable(&self, thread: ChannelId) -> bool {
        self.read(|inner| inner.state.thread_unavailable(thread))
    }

    /// A guild's channels and categories without threads, in no particular order.
    pub fn guild_channels(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.guild_channels(guild))
    }

    /// A guild's channel list as Discord shows it: [`display_order`], only channels the
    /// user can view, the categories holding them, and empty categories the user can
    /// view. Each channel is followed by its listed threads, the last joined first: those the
    /// user joined that aren't archived and are inside their inactivity window
    /// ([`Channel::window_end`]), unread, or with mentions. The parent's mute and "hide muted
    /// channels" don't change which threads are listed.
    ///
    /// [`display_order`]: super::display_order
    pub fn channel_list(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        let now = self.now_millis();
        self.read(|inner| {
            let state = &inner.state;
            let ordered = order::display_order(&state.guild_channels(guild));
            let visible = order::visible_channels(ordered, |channel| {
                state
                    .permissions(channel.id, now)
                    .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
            });
            let mut list = Vec::with_capacity(visible.len());
            for channel in visible {
                let threads = state.listed_threads(channel.id);
                list.push(channel);
                list.extend(threads);
            }
            list
        })
    }

    /// The threads the user has joined in a guild.
    pub fn threads(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.threads(guild))
    }

    /// DMs and group DMs, in no particular order.
    pub fn private_channels(&self) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.private_channels())
    }

    /// DMs and group DMs as the DM list shows them: the latest conversation first. One
    /// without messages counts from when it was created.
    pub fn private_channel_list(&self) -> Vec<Arc<Channel>> {
        let listed = self
            .read(|inner| inner.state.private_channels())
            .into_iter()
            .filter(|channel| !channel.message_request && !channel.spam)
            .collect();
        order::private_channel_order(listed)
    }

    /// The current user's permissions in a guild channel or thread; in a thread,
    /// `SEND_MESSAGES` means the user may send there. `None` for DMs and for channels,
    /// guilds or memberships the store doesn't know.
    pub fn permissions(&self, channel: ChannelId) -> Option<Permissions> {
        let now = self.now_millis();
        self.read(|inner| inner.state.permissions(channel, now))
    }

    /// The longest message the current user may send: 4,000 characters with Nitro, else 2,000.
    pub fn message_length_limit(&self) -> usize {
        self.read(|inner| inner.state.length_limit())
    }

    /// The channel's slowmode as it applies to the current user; `None` without slowmode, in
    /// DMs and group DMs, and for unknown channels.
    pub fn slowmode(&self, channel: ChannelId) -> Option<Slowmode> {
        let now = self.now_millis();
        self.read(|inner| {
            inner
                .state
                .slowmode(channel, now, Instant::now(), SystemTime::now())
        })
    }

    /// The loaded messages of a viewed channel; `None` if it isn't viewed.
    pub fn messages(&self, channel: ChannelId) -> Option<MessageWindow> {
        self.read(|inner| inner.state.messages(channel))
    }

    /// The message `thread` started from, once its window holds the thread's first message;
    /// `None` for a thread started without one, or before that message is loaded.
    pub fn starter_message(&self, thread: ChannelId) -> Option<StarterMessage> {
        self.read(|inner| inner.state.starter_message(thread))
    }

    pub fn message(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.read(|inner| inner.state.message(channel, id))
    }

    #[cfg(test)]
    pub(crate) fn ready_thread(&self) -> Option<std::thread::ThreadId> {
        self.shared
            .ready_thread
            .lock()
            .ok()
            .and_then(|thread| *thread)
    }

    #[cfg(test)]
    pub(crate) fn journaling(&self) -> bool {
        self.read(|inner| inner.state.journaling())
    }

    #[cfg(test)]
    pub(crate) fn subscriber_count(&self) -> usize {
        self.read(|inner| inner.subscribers.len())
    }

    #[cfg(test)]
    pub(crate) fn park_next_conversion(
        &self,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (parked, on_parked) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        if let Ok(mut park) = self.shared.park.lock() {
            *park = Some((parked, on_release));
        }
        (on_parked, release)
    }

    #[cfg(test)]
    pub(crate) fn delay_next_write(&self, delay: std::time::Duration) {
        if let Ok(mut slow) = self.shared.slow_write.lock() {
            *slow = Some(delay);
        }
    }

    #[cfg(test)]
    pub(crate) fn record_holds(&self) {
        if let Ok(mut holds) = self.shared.holds.lock() {
            *holds = Some(Vec::new());
        }
    }

    #[cfg(test)]
    pub(crate) fn take_holds(&self) -> Vec<(&'static str, std::time::Duration)> {
        self.shared
            .holds
            .lock()
            .ok()
            .and_then(|mut holds| holds.as_mut().map(std::mem::take))
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn long_holds(&self) -> Vec<(&'static str, std::time::Duration)> {
        self.shared
            .long_holds
            .lock()
            .map(|holds| holds.clone())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn slow_down(&self) {
        let delay = self
            .shared
            .slow_write
            .lock()
            .ok()
            .and_then(|mut slow| slow.take());
        if let Some(delay) = delay {
            std::thread::sleep(delay);
        }
    }

    #[cfg(test)]
    fn park(&self) {
        let park = self
            .shared
            .park
            .lock()
            .ok()
            .and_then(|mut park| park.take());
        if let Some((parked, release)) = park {
            let _ = parked.send(());
            let _ = release.recv();
        }
    }
}

#[cfg(test)]
mod tests;
