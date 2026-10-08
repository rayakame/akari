use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, mpsc};

use super::apply::{Entities, State};
use super::events::{ConnectionState, StoreEvent};
use super::types::{Channel, CurrentUser, Guild, Member, Message, User};
#[cfg(test)]
use super::windows::DEFAULT_LIMITS;
use super::windows::{MessageWindow, WindowLimits};
use crate::backlog::Backlog;
use crate::gateway::{DispatchEvent, Ready};
use crate::model::{ChannelId, GuildId, MessageId, Permissions, UserId};

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

    fn write(&self, write: impl FnOnce(&mut Inner, &mut Vec<StoreEvent>)) {
        let mut inner = self
            .shared
            .inner
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let mut events = Vec::new();
        write(&mut inner, &mut events);
        inner.publish(events);
    }

    pub(crate) fn apply(&self, event: DispatchEvent) {
        match event {
            DispatchEvent::Ready(ready) => {
                let next = self.prepare_ready(*ready);
                self.replace(next);
            }
            event => self.write(|inner, events| inner.state.apply(event, events)),
        }
    }

    // Converting a large READY takes milliseconds; readers mustn't wait for it, so it
    // happens before the write lock is taken.
    pub(crate) fn prepare_ready(&self, ready: Ready) -> Entities {
        let next = Entities::from_ready(ready);
        #[cfg(test)]
        {
            if let Ok(mut thread) = self.shared.ready_thread.lock() {
                *thread = Some(std::thread::current().id());
            }
            self.park();
        }
        next
    }

    pub(crate) fn replace(&self, next: Entities) {
        self.write(|inner, events| inner.state.replace(next, events));
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
        self.write(|inner, events| {
            let closed = matches!(inner.connection, ConnectionState::Closed { .. });
            if !closed && !same(&inner.connection, &connection) && allowed() {
                inner.connection = connection.clone();
                events.push(StoreEvent::Connection(connection));
            }
        });
    }

    pub(crate) fn begin_connecting(&self) {
        self.write(|inner, events| {
            if matches!(inner.connection, ConnectionState::Offline) {
                inner.connection = ConnectionState::Connecting;
                events.push(StoreEvent::Connection(ConnectionState::Connecting));
            }
        });
    }

    pub(crate) fn view_channel(&self, channel: ChannelId) {
        self.write(|inner, events| inner.state.view_channel(channel, events));
    }

    pub(crate) fn finish(&self) {
        self.write(|inner, _| {
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
        self.write(|inner, _| {
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

    /// The threads the user has joined in a guild.
    pub fn threads(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.threads(guild))
    }

    /// DMs and group DMs.
    pub fn private_channels(&self) -> Vec<Arc<Channel>> {
        self.read(|inner| inner.state.private_channels())
    }

    /// The current user's permissions in a guild channel or thread; in a thread,
    /// `SEND_MESSAGES` means the user may send there. `None` for DMs and for channels,
    /// guilds or memberships the store doesn't know.
    pub fn permissions(&self, channel: ChannelId) -> Option<Permissions> {
        let now = now_millis();
        self.read(|inner| inner.state.permissions(channel, now))
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
    pub(crate) fn park_next_ready(
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
