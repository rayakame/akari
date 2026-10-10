use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, mpsc};

use super::apply::{Entities, ReadyDiff, State};
use super::events::{ConnectionState, StoreEvent};
use super::order;
use super::send::Slowmode;
use super::types::{Channel, CurrentUser, Guild, Member, Message, User};
#[cfg(test)]
use super::windows::DEFAULT_LIMITS;
use super::windows::{LoadKind, LoadTicket, MessageWindow, WindowLimits};
use crate::backlog::Backlog;
use crate::gateway::{DispatchEvent, GatewayGuild, Ready};
use crate::model::{self, ChannelId, GuildId, MessageId, Permissions, UserId};

// Main-thread readers (SwiftUI, AppKit) wait at most this long for a write.
const LONG_HOLD: Duration = Duration::from_millis(4);

pub(crate) struct PreparedReady {
    entities: Entities,
    diff: Option<ReadyDiff>,
}

/// An account's state, kept current by its gateway connection. Cheap to clone. Reads
/// return snapshots: values later changes never touch.
#[derive(Clone)]
pub struct Store {
    shared: Arc<Shared>,
}

struct Shared {
    inner: RwLock<Inner>,
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

    #[cfg(test)]
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

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

impl Store {
    pub(crate) fn new(limits: WindowLimits) -> Self {
        Self {
            shared: Arc::new(Shared {
                inner: RwLock::new(Inner {
                    state: State::with_limits(limits),
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

    fn read<T>(&self, read: impl FnOnce(&Inner) -> T) -> T {
        let inner = self
            .shared
            .inner
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        read(&inner)
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
        let value = write(&mut inner, &mut events);
        inner.publish(events);
        #[cfg(test)]
        self.slow_down();
        let held = taken.elapsed();
        drop(inner);
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

    pub(crate) fn apply(&self, event: DispatchEvent) {
        match event {
            DispatchEvent::Ready(ready) => {
                let next = self.prepare_ready(*ready);
                self.replace(next);
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
                    });
                }
                unavailable => {
                    let event = DispatchEvent::GuildCreate(Box::new(unavailable));
                    self.write("GUILD_CREATE", |inner, events| {
                        inner.state.apply(event, events);
                    });
                }
            },
            event => {
                let name = event.name();
                self.write(name, |inner, events| inner.state.apply(event, events));
            }
        }
    }

    // Converting and diffing a large READY takes milliseconds, so readers mustn't wait for
    // it. Only the pump changes entities, so the diff still holds at the swap.
    pub(crate) fn prepare_ready(&self, ready: Ready) -> PreparedReady {
        let entities = Entities::from_ready(ready);
        let diff = self.read(|inner| inner.state.ready_diff(&entities));
        #[cfg(test)]
        {
            if let Ok(mut thread) = self.shared.ready_thread.lock() {
                *thread = Some(std::thread::current().id());
            }
            self.park();
        }
        PreparedReady { entities, diff }
    }

    pub(crate) fn replace(&self, next: PreparedReady) {
        let old = self.write("READY", |inner, events| {
            inner.state.swap(next.entities, next.diff, events)
        });
        drop(old);
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

    pub(crate) fn begin_load(&self, channel: ChannelId, kind: LoadKind) -> Option<LoadTicket> {
        let mut ticket = None;
        self.write("load", |inner, events| {
            ticket = inner.state.begin_load(channel, kind, events);
        });
        ticket
    }

    pub(crate) fn finish_load(
        &self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        reached_end: bool,
    ) {
        self.write("load", |inner, events| {
            inner.state.finish_load(ticket, page, reached_end, events);
        });
    }

    pub(crate) fn abort_load(&self, ticket: LoadTicket) {
        self.write("load", |inner, events| {
            inner.state.abort_load(ticket, events)
        });
    }

    pub(crate) fn stale_channels(&self) -> Vec<ChannelId> {
        self.read(|inner| inner.state.stale_channels())
    }

    pub(crate) fn viewed_channels(&self) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.viewed_channels())
    }

    pub(crate) fn queue_message(&self, channel: ChannelId, message: Arc<Message>) {
        let now = now_millis();
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
        let now = now_millis();
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
        let now = now_millis();
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

    /// Available guilds in server list order. Akari doesn't read the user's own order and
    /// folders yet; until it does, the most recently joined guild comes first.
    pub fn guild_list(&self) -> Vec<Arc<Guild>> {
        self.read(|inner| {
            let state = &inner.state;
            order::guild_order(
                state
                    .guilds()
                    .into_iter()
                    .map(|guild| {
                        let joined = state
                            .current_member(guild.id)
                            .and_then(|member| member.joined_at);
                        (guild, joined)
                    })
                    .collect(),
            )
        })
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

    /// A guild channel, category, thread, DM or group DM.
    pub fn channel(&self, id: ChannelId) -> Option<Arc<Channel>> {
        self.read(|inner| inner.state.channel(id))
    }

    /// A guild's channels and categories without threads, in no particular order.
    pub fn guild_channels(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.guild_channels(guild))
    }

    /// A guild's channel list as Discord shows it: [`display_order`], only channels the
    /// user can view, the categories holding them, and empty categories the user can
    /// view. Threads are left out.
    ///
    /// [`display_order`]: super::display_order
    pub fn channel_list(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        let now = now_millis();
        self.read(|inner| {
            let state = &inner.state;
            let ordered = order::display_order(&state.guild_channels(guild));
            order::visible_channels(ordered, |channel| {
                state
                    .permissions(channel.id, now)
                    .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
            })
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
        order::private_channel_order(self.read(|inner| inner.state.private_channels()))
    }

    /// The current user's permissions in a guild channel or thread; in a thread,
    /// `SEND_MESSAGES` means the user may send there. `None` for DMs and for channels,
    /// guilds or memberships the store doesn't know.
    pub fn permissions(&self, channel: ChannelId) -> Option<Permissions> {
        let now = now_millis();
        self.read(|inner| inner.state.permissions(channel, now))
    }

    /// The longest message the current user may send: 4,000 characters with Nitro, else 2,000.
    pub fn message_length_limit(&self) -> usize {
        self.read(|inner| inner.state.length_limit())
    }

    /// The channel's slowmode as it applies to the current user; `None` without slowmode, in
    /// DMs and group DMs, and for unknown channels.
    pub fn slowmode(&self, channel: ChannelId) -> Option<Slowmode> {
        let now = now_millis();
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
