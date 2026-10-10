use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::gateway::session::Timing;
use crate::gateway::{
    ConnectionEvent, DispatchEvent, Gateway, GatewayCommand, GatewayError, GuildSubscription,
    MemberLists, PresenceStatus, SendError,
};
use crate::model::{ChannelId, GuildId, MessageId};
use crate::rest::{AccountRest, CreateMessage, Page, Query, RequestError};
use crate::state::{
    ConnectionState, Cursor, LoadKind, LoadTicket, Message, Store, WindowLimits, message_length,
};
use crate::{DiscordClient, Token};

const REFRESH_LIMIT: u8 = 100;
const JUMP_LIMIT: u8 = 50;
const REFRESH_RETRIES: u32 = 3;
// Short in tests, so retries don't slow the suite down.
const SERVER_ERROR_RETRY: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};

/// A logged-in account: its gateway connection and the [`Store`] that connection keeps
/// current. Dropping it ends the session like [`Account::close`].
pub struct Account {
    shared: Arc<Shared>,
}

/// Which messages [`Account::load_messages`] loads. `limit` is clamped to 1–100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageLoad {
    /// The newest messages: fills a window, refreshes a stale one, or jumps to the present.
    Latest { limit: u8 },
    /// Before the window's first message.
    Older { limit: u8 },
    /// After the window's last message, towards the present.
    Newer { limit: u8 },
    /// Replaces the window with the messages around `id`.
    Around { id: MessageId, limit: u8 },
}

impl MessageLoad {
    fn parts(self) -> (LoadKind, u8) {
        let (kind, limit) = match self {
            Self::Latest { limit } => (LoadKind::Latest, limit),
            Self::Older { limit } => (LoadKind::Older, limit),
            Self::Newer { limit } => (LoadKind::Newer, limit),
            Self::Around { id, limit } => (LoadKind::Around(id), limit),
        };
        (kind, limit.clamp(1, 100))
    }
}

pub(crate) struct Shared {
    gateway: Gateway,
    store: Store,
    rest: AccountRest,
    nonces: Nonces,
    status: Mutex<Status>,
    // What op 37 last asked for per guild in this session.
    subscribed: Mutex<BTreeMap<GuildId, GuildSubscription>>,
    member_lists: AtomicBool,
    runtime: tokio::runtime::Handle,
}

// A new session starts at `unknown`, so the chosen status is sent after every READY; after
// RESUMED only if it didn't go out before.
#[derive(Default)]
struct Status {
    chosen: Option<PresenceStatus>,
    sent: bool,
}

// Like the official client: a snowflake of the current time, strictly increasing, so it
// doubles as the pending message's provisional ID.
#[derive(Default)]
pub(crate) struct Nonces(Mutex<u64>);

impl Nonces {
    pub(crate) fn next(&self, unix_millis: i64) -> MessageId {
        let now = MessageId::from_unix_millis(unix_millis, 0).get();
        let mut last = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *last = (*last + 1).max(now);
        MessageId::new(*last)
    }
}

struct Queued {
    pending: MessageId,
    detached: bool,
}

// A load dropped before its page arrived, e.g. by a cancelled task, would hold live messages
// back forever.
struct Unfinished<'a> {
    store: &'a Store,
    ticket: Option<LoadTicket>,
}

impl Drop for Unfinished<'_> {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.take() {
            self.store.abort_load(ticket);
        }
    }
}

// A send dropped before Discord answered, e.g. by a cancelled task, stays retryable instead of
// pending forever.
struct Unanswered<'a> {
    store: &'a Store,
    channel: ChannelId,
    pending: Option<MessageId>,
}

impl Drop for Unanswered<'_> {
    fn drop(&mut self) {
        if let Some(pending) = self.pending {
            self.store.fail_message(self.channel, pending);
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

impl Shared {
    async fn load(
        self: &Arc<Self>,
        channel: ChannelId,
        kind: LoadKind,
        limit: u8,
    ) -> Result<(), RequestError> {
        let Some(ticket) = self.store.begin_load(channel, kind) else {
            return Ok(());
        };
        let mut unfinished = Unfinished {
            store: &self.store,
            ticket: Some(ticket),
        };
        self.subscribe_guild_of(channel);
        let query = match ticket.cursor {
            Cursor::Latest => Query::Latest,
            Cursor::Before(id) => Query::Before(id),
            Cursor::After(id) => Query::After(id),
            Cursor::Around(id) => Query::Around(id),
        };
        // A flaky network right after a reconnect mustn't detach every open window.
        let retries = if kind == LoadKind::Refresh {
            REFRESH_RETRIES
        } else {
            0
        };
        let mut attempt = 0;
        let loaded = loop {
            match self.rest.list_messages(channel, query, limit).await {
                Err(err) if attempt < retries && err.is_transient() => {
                    tokio::time::sleep(SERVER_ERROR_RETRY * 2u32.pow(attempt)).await;
                    attempt += 1;
                }
                loaded => break loaded,
            }
        };
        unfinished.ticket = None;
        match loaded {
            Ok(Page { messages, received }) => {
                let reached_end = received < usize::from(limit);
                self.store.finish_load(ticket, messages, reached_end);
                Ok(())
            }
            Err(err) => {
                self.store.abort_load(ticket);
                self.failed(&err);
                Err(err)
            }
        }
    }

    // A 401 means the token is gone, like the gateway's 4004: the UI has to log in again.
    fn failed(&self, err: &RequestError) {
        if matches!(err, RequestError::Unauthorized) {
            let error = Some(Arc::new(GatewayError::AuthenticationFailed));
            self.store.set_connection(ConnectionState::Closed { error });
            self.gateway.close();
        }
    }

    fn view(self: &Arc<Self>, channel: ChannelId) {
        self.store.view_channel(channel);
        self.subscribe_guild_of(channel);
    }

    // Like the official client: a guild is subscribed once one of its channels is opened, and
    // in large guilds so is that channel's member list.
    fn subscribe_guild_of(self: &Arc<Self>, channel: ChannelId) {
        if self.store.channel(channel).is_none() {
            tracing::debug!(
                target: "akari_core::subscriptions",
                channel = channel.get(),
                "not subscribing: the channel isn't in the state"
            );
            return;
        }
        if !matches!(self.store.connection(), ConnectionState::Online) {
            return;
        }
        let changed = self.subscription_changes();
        if !changed.is_empty() {
            let shared = self.clone();
            self.runtime.spawn(async move {
                for command in GatewayCommand::subscribe_guilds(&changed) {
                    let _ = shared.send(command).await;
                }
            });
        }
    }

    // The entries that differ from what this session was sent, marked as sent.
    fn subscription_changes(&self) -> Vec<GuildSubscription> {
        let mut subscribed = self
            .subscribed
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut wanted = self.wanted_subscriptions();
        // Member lists of channels no longer viewed are dropped; the guild stays subscribed.
        for (guild, sent) in subscribed.iter() {
            if sent.member_lists.is_some() && !wanted.contains_key(guild) {
                let mut dropped = GuildSubscription::new(*guild);
                dropped.member_lists = Some(MemberLists::default());
                wanted.insert(*guild, dropped);
            }
        }
        let mut changed = Vec::new();
        for entry in wanted.into_values() {
            if subscribed.get(&entry.guild_id) != Some(&entry) {
                subscribed.insert(entry.guild_id, entry.clone());
                changed.push(entry);
            }
        }
        changed
    }

    // A new or resumed session starts unsubscribed. Called before it goes online, so views
    // from then on and the re-send after it never send the same entry twice.
    fn forget_subscriptions(&self) {
        self.subscribed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn wanted_subscriptions(&self) -> BTreeMap<GuildId, GuildSubscription> {
        let mut wanted: BTreeMap<GuildId, GuildSubscription> = BTreeMap::new();
        for channel in self.store.viewed_channels() {
            let Some(guild) = channel.guild_id else {
                continue;
            };
            let entry = wanted
                .entry(guild)
                .or_insert_with(|| GuildSubscription::new(guild));
            let large = self.store.guild(guild).is_some_and(|guild| guild.large);
            if !large || !self.member_lists.load(Ordering::Relaxed) {
                continue;
            }
            let lists = entry.member_lists.get_or_insert_default();
            match channel.parent_id.filter(|_| channel.is_thread()) {
                Some(parent) => {
                    lists.channels.push(parent);
                    lists.threads.push(channel.id);
                }
                None => lists.channels.push(channel.id),
            }
        }
        for lists in wanted
            .values_mut()
            .filter_map(|entry| entry.member_lists.as_mut())
        {
            lists.channels.sort_unstable();
            lists.channels.dedup();
            lists.threads.sort_unstable();
            lists.threads.dedup();
        }
        wanted
    }

    // The official client sends its subscriptions again after READY and after RESUMED.
    // Channels viewed while offline count too.
    async fn resubscribe(&self) {
        for command in GatewayCommand::subscribe_guilds(&self.subscription_changes()) {
            let _ = self.send(command).await;
        }
    }

    fn queue(self: &Arc<Self>, channel: ChannelId, content: &str) -> Result<Queued, RequestError> {
        let author = self
            .store
            .current_user()
            .ok_or(RequestError::InvalidRequest)?;
        if content.trim().is_empty() {
            return Err(RequestError::InvalidRequest);
        }
        let limit = self.store.message_length_limit();
        if message_length(content) > limit {
            return Err(RequestError::TooLong { limit });
        }
        let detached = match self.store.messages(channel) {
            Some(window) => !window.latest,
            None => {
                self.view(channel);
                false
            }
        };
        let now = now_millis();
        let pending = self.nonces.next(now);
        let message = Message::pending(
            pending,
            channel,
            Arc::new(author.user.clone()),
            content.to_owned(),
            now,
        );
        self.store.queue_message(channel, Arc::new(message));
        self.store.start_cooldown(channel, pending);
        Ok(Queued { pending, detached })
    }

    async fn deliver(
        &self,
        channel: ChannelId,
        pending: MessageId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        let mut unanswered = Unanswered {
            store: &self.store,
            channel,
            pending: Some(pending),
        };
        let body = CreateMessage::new(content, pending.get().to_string());
        let delivered = match self.rest.create_message(channel, &body).await {
            Ok(message) => {
                let id = message.id;
                self.store.confirm_message(channel, pending, message);
                Ok(id)
            }
            Err(err) => {
                // The core can't count what Discord refused; the 0 asks for our own limit.
                let err = match err {
                    RequestError::TooLong { limit: 0 } => RequestError::TooLong {
                        limit: self.store.message_length_limit(),
                    },
                    err => err,
                };
                self.store.fail_message(channel, pending);
                match &err {
                    RequestError::RateLimited {
                        retry_after: Some(wait),
                    } => self.store.hold_cooldown(channel, *wait),
                    _ => self.store.drop_cooldown(channel, pending),
                }
                self.failed(&err);
                Err(err)
            }
        };
        unanswered.pending = None;
        delivered
    }

    async fn send_status(&self, new_session: bool) {
        let status = {
            let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
            if new_session {
                status.sent = false;
            }
            if status.sent {
                return;
            }
            status.chosen
        };
        let Some(status) = status else {
            return;
        };
        if self.send_presence(status).await.is_ok() {
            self.mark_sent(status);
        }
    }

    pub(crate) async fn send(&self, command: GatewayCommand) -> Result<(), SendError> {
        self.gateway.send(command).await
    }

    async fn send_presence(&self, status: PresenceStatus) -> Result<(), SendError> {
        self.send(GatewayCommand::UpdatePresence { status }).await
    }

    fn mark_sent(&self, sent: PresenceStatus) {
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if status.chosen == Some(sent) {
            status.sent = true;
        }
    }

    async fn refresh_stale(self: &Arc<Self>) {
        for channel in self.store.stale_channels() {
            let refreshed = self.load(channel, LoadKind::Refresh, REFRESH_LIMIT).await;
            if refreshed.is_err() && self.rest.is_unauthorized() {
                return;
            }
        }
    }

    fn close(&self) {
        self.gateway.close();
        self.rest.close();
    }
}

impl Account {
    pub(crate) fn start(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
    ) -> Result<Self, GatewayError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| GatewayError::NoRuntime)?;
        let rest = AccountRest::new(client.clone(), token.clone(), SERVER_ERROR_RETRY);
        let shared = Arc::new(Shared {
            gateway: Gateway::start(client, token, timing)?,
            store: Store::new(limits),
            rest,
            nonces: Nonces::default(),
            status: Mutex::default(),
            subscribed: Mutex::default(),
            member_lists: AtomicBool::new(true),
            runtime: runtime.clone(),
        });
        // Built before the task starts, so it runs even if the task is never polled.
        let finish = Finish::new(shared.clone());
        runtime.spawn(pump(finish));
        Ok(Self { shared })
    }

    /// The account's state. It stays readable after the account is closed.
    pub fn store(&self) -> &Store {
        &self.shared.store
    }

    /// Connects, or resumes after [`Account::disconnect`]. Does nothing while connected.
    /// Fails with [`GatewayError::Closed`] after `close()` or a fatal error.
    pub fn connect(&self) -> Result<(), GatewayError> {
        self.shared.gateway.connect()?;
        self.shared.store.begin_connecting();
        Ok(())
    }

    /// Closes the connection but keeps the session, e.g. while the app is suspended; the
    /// next `connect()` resumes it.
    pub fn disconnect(&self) {
        self.shared.gateway.disconnect();
        self.shared.store.set_connection(ConnectionState::Offline);
    }

    /// Ends the session. Subscriptions end once the connection is closed.
    pub fn close(&self) {
        self.shared.close();
    }

    /// Marks the channel as viewed: the store keeps its messages and adds new ones from
    /// the gateway. Past 10 viewed channels, the least recently viewed one without pending or
    /// failed messages loses its messages.
    pub fn view_channel(&self, channel: ChannelId) {
        self.shared.view(channel);
    }

    /// Loads messages into the channel's window within Discord's rate limits. `Latest` and
    /// `Around` view the channel first; `Older` and `Newer` continue a window and do nothing
    /// without one. A 401 closes the account like a rejected token.
    pub async fn load_messages(
        &self,
        channel: ChannelId,
        load: MessageLoad,
    ) -> Result<(), RequestError> {
        let (kind, limit) = load.parts();
        self.shared.load(channel, kind, limit).await
    }

    /// Shows the message as pending at once and sends it. Resolves with Discord's ID; on an
    /// error the pending message is marked failed and stays until retried or discarded.
    pub async fn send_message(
        &self,
        channel: ChannelId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        let shared = &self.shared;
        let Queued { pending, detached } = shared.queue(channel, &content)?;
        // Like the official client, a detached window jumps to the present; a stale one waits
        // for its refresh. The send doesn't wait for the jump.
        if detached {
            let (jumped, delivered) = tokio::join!(
                shared.load(channel, LoadKind::Latest, JUMP_LIMIT),
                shared.deliver(channel, pending, content),
            );
            if let Err(err) = jumped {
                tracing::debug!(error = %err, "couldn't jump to the present before sending");
            }
            return delivered;
        }
        shared.deliver(channel, pending, content).await
    }

    /// Dev only: sends like [`Account::send_message`], then posts the same body with the same
    /// nonce again, as a retry after a lost response would. Returns what Discord answered to
    /// each; the repeat isn't tracked in the store.
    #[cfg(feature = "repeat-nonce")]
    pub async fn send_message_twice(
        &self,
        channel: ChannelId,
        content: String,
    ) -> Result<(MessageId, Result<MessageId, RequestError>), RequestError> {
        let shared = &self.shared;
        let Queued { pending, .. } = shared.queue(channel, &content)?;
        let first = shared.deliver(channel, pending, content.clone()).await?;
        let body = CreateMessage::new(content, pending.get().to_string());
        let repeat = shared
            .rest
            .create_message(channel, &body)
            .await
            .map(|message| message.id);
        Ok((first, repeat))
    }

    /// Dev only: op 37 leaves member lists out, sending only the guild flags. Call it before
    /// `connect()`.
    #[cfg(feature = "flags-only")]
    pub fn subscribe_flags_only(&self) {
        self.shared.member_lists.store(false, Ordering::Relaxed);
    }

    /// Sends a failed message again with the same nonce.
    pub async fn retry_message(
        &self,
        channel: ChannelId,
        pending: MessageId,
    ) -> Result<MessageId, RequestError> {
        let message = self
            .shared
            .store
            .retry_message(channel, pending)
            .ok_or(RequestError::InvalidRequest)?;
        self.shared.store.start_cooldown(channel, pending);
        self.shared
            .deliver(channel, pending, message.content.to_string())
            .await
    }

    /// Drops a failed message.
    pub fn discard_message(&self, channel: ChannelId, pending: MessageId) {
        self.shared.store.discard_message(channel, pending);
    }

    /// Sets this session's status now if online, and again after every new session.
    pub async fn set_status(&self, status: PresenceStatus) -> Result<(), SendError> {
        *self
            .shared
            .status
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Status {
            chosen: Some(status),
            sent: false,
        };
        if matches!(self.shared.store.connection(), ConnectionState::Online) {
            self.shared.send_presence(status).await?;
            self.shared.mark_sent(status);
        }
        Ok(())
    }
}

// Closes the account however the pump ends: normally, by a panic, or because the runtime
// shut down and dropped the task. Without it, subscribers would wait forever.
pub(crate) struct Finish {
    shared: Arc<Shared>,
    ended: Option<Option<Arc<GatewayError>>>,
}

impl Finish {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            ended: None,
        }
    }
}

impl Drop for Finish {
    fn drop(&mut self) {
        // close() does nothing once the gateway has ended, so it runs on every path.
        self.shared.close();
        let error = self
            .ended
            .take()
            .unwrap_or_else(|| Some(Arc::new(GatewayError::Stopped)));
        self.shared
            .store
            .set_connection(ConnectionState::Closed { error });
        self.shared.store.finish();
    }
}

impl Drop for Account {
    fn drop(&mut self) {
        self.shared.close();
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("connection", &self.shared.store.connection())
            .finish_non_exhaustive()
    }
}

// The gateway buffers its events and the store never waits for subscribers, so nothing
// here waits on a consumer.
async fn pump(mut finish: Finish) {
    let shared = finish.shared.clone();
    let connecting = || shared.gateway.wants_connection();
    let error = loop {
        match shared.gateway.next().await {
            // A large READY takes milliseconds to convert; that mustn't hold a runtime worker.
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Ready(ready))) => {
                let started = std::time::Instant::now();
                let store = shared.store.clone();
                match tokio::task::spawn_blocking(move || store.prepare_ready(*ready)).await {
                    Ok(next) => {
                        shared.store.replace(next);
                        let convert_ms = crate::millis(started.elapsed());
                        tracing::info!(convert_ms, "READY applied");
                        shared.forget_subscriptions();
                        shared
                            .store
                            .set_connection_if(ConnectionState::Online, connecting);
                        let refresh = shared.clone();
                        tokio::spawn(async move { refresh.refresh_stale().await });
                        let status = shared.clone();
                        tokio::spawn(async move { status.send_status(true).await });
                        let subscriptions = shared.clone();
                        tokio::spawn(async move { subscriptions.resubscribe().await });
                    }
                    Err(_) => break Some(Arc::new(GatewayError::Stopped)),
                }
            }
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Resumed)) => {
                shared.forget_subscriptions();
                on_event(
                    &shared.store,
                    ConnectionEvent::Dispatch(DispatchEvent::Resumed),
                    &connecting,
                );
                let status = shared.clone();
                tokio::spawn(async move { status.send_status(false).await });
                let subscriptions = shared.clone();
                tokio::spawn(async move { subscriptions.resubscribe().await });
            }
            Ok(event) => on_event(&shared.store, event, &connecting),
            Err(GatewayError::Closed) => break None,
            Err(error) => break Some(Arc::new(error)),
        }
    };
    finish.ended = Some(error);
}

// Queued events mustn't make a disconnected account look connected. disconnect() idles the
// gateway before it takes the store's lock, so `connecting` checked under that lock sees it.
fn on_event(store: &Store, event: ConnectionEvent, connecting: &dyn Fn() -> bool) {
    match event {
        ConnectionEvent::Dispatch(event) => {
            let ready = matches!(event, DispatchEvent::Ready(_) | DispatchEvent::Resumed);
            store.apply(event);
            if ready {
                store.set_connection_if(ConnectionState::Online, connecting);
            }
        }
        ConnectionEvent::Reconnecting { .. } => {
            store.set_connection_if(ConnectionState::Connecting, connecting);
        }
        #[cfg(feature = "capture")]
        ConnectionEvent::CapturedReady(_) => {}
    }
}

#[cfg(test)]
mod tests;
