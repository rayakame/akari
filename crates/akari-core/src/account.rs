use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::gateway::session::Timing;
use crate::gateway::{
    ConnectionEvent, DispatchEvent, Gateway, GatewayCommand, GatewayError, PresenceStatus,
    SendError,
};
use crate::model::{ChannelId, GuildId, MessageId};
use crate::rest::{AccountRest, CreateMessage, Query, RequestError};
use crate::state::{ConnectionState, Cursor, LoadKind, Message, Store, WindowLimits};
use crate::{DiscordClient, Token};

const REFRESH_LIMIT: u8 = 100;
const JUMP_LIMIT: u8 = 50;
// Short in tests, so retried 502s don't slow the suite down.
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
    // Guilds subscribed with op 37 in this session.
    subscribed: Mutex<BTreeSet<GuildId>>,
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
        self.subscribe_guild_of(channel);
        let query = match ticket.cursor {
            Cursor::Latest => Query::Latest,
            Cursor::Before(id) => Query::Before(id),
            Cursor::After(id) => Query::After(id),
            Cursor::Around(id) => Query::Around(id),
        };
        match self.rest.list_messages(channel, query, limit).await {
            Ok(page) => {
                self.store.finish_load(ticket, page, usize::from(limit));
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

    // Discord jumps to the present before sending; a stale window just waits for its refresh.
    fn view(self: &Arc<Self>, channel: ChannelId) {
        self.store.view_channel(channel);
        self.subscribe_guild_of(channel);
    }

    // Like the official client: a guild is subscribed once one of its channels is opened.
    fn subscribe_guild_of(self: &Arc<Self>, channel: ChannelId) {
        let Some(guild) = self
            .store
            .channel(channel)
            .and_then(|channel| channel.guild_id)
        else {
            return;
        };
        if !matches!(self.store.connection(), ConnectionState::Online) {
            return;
        }
        let new = self
            .subscribed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(guild);
        if new {
            let shared = self.clone();
            self.runtime.spawn(async move {
                let _ = shared
                    .send(GatewayCommand::SubscribeGuilds {
                        guilds: vec![guild],
                    })
                    .await;
            });
        }
    }

    // Subscriptions belong to a session; the official client sends them again after READY
    // and after RESUMED.
    async fn resubscribe(&self, new_session: bool) {
        let guilds: Vec<GuildId> = {
            let mut subscribed = self
                .subscribed
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if new_session {
                *subscribed = self.store.viewed_guilds();
            }
            subscribed.iter().copied().collect()
        };
        for command in GatewayCommand::subscribe_guilds(&guilds) {
            let _ = self.send(command).await;
        }
    }

    async fn prepare_send(self: &Arc<Self>, channel: ChannelId) -> Result<(), RequestError> {
        match self.store.messages(channel) {
            None => self.view(channel),
            Some(window) if !window.latest => {
                self.load(channel, LoadKind::Latest, JUMP_LIMIT).await?;
            }
            Some(_) => {}
        }
        Ok(())
    }

    async fn deliver(
        &self,
        channel: ChannelId,
        pending: MessageId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        let body = CreateMessage::new(content, pending.get().to_string());
        match self.rest.create_message(channel, &body).await {
            Ok(message) => {
                let id = message.id;
                self.store.confirm_message(channel, pending, message);
                Ok(id)
            }
            Err(err) => {
                self.store.fail_message(channel, pending);
                self.failed(&err);
                Err(err)
            }
        }
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
    /// the gateway. Past 10 viewed channels, the least recently viewed loses its messages.
    pub fn view_channel(&self, channel: ChannelId) {
        self.shared.view(channel);
    }

    /// Loads messages into the channel's window, viewing it first, within Discord's rate
    /// limits. A 401 closes the account like a rejected token.
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
        let author = shared
            .store
            .current_user()
            .ok_or(RequestError::InvalidRequest)?;
        if content.trim().is_empty() {
            return Err(RequestError::InvalidRequest);
        }
        shared.prepare_send(channel).await?;
        let now = now_millis();
        let pending = shared.nonces.next(now);
        let message = Message::pending(
            pending,
            channel,
            Arc::new(author.user.clone()),
            content.clone(),
            now,
        );
        shared.store.queue_message(channel, Arc::new(message));
        shared.deliver(channel, pending, content).await
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
                let store = shared.store.clone();
                match tokio::task::spawn_blocking(move || store.prepare_ready(*ready)).await {
                    Ok(next) => {
                        shared.store.replace(next);
                        shared
                            .store
                            .set_connection_if(ConnectionState::Online, connecting);
                        let refresh = shared.clone();
                        tokio::spawn(async move { refresh.refresh_stale().await });
                        let status = shared.clone();
                        tokio::spawn(async move { status.send_status(true).await });
                        let subscriptions = shared.clone();
                        tokio::spawn(async move { subscriptions.resubscribe(true).await });
                    }
                    Err(_) => break Some(Arc::new(GatewayError::Stopped)),
                }
            }
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Resumed)) => {
                on_event(
                    &shared.store,
                    ConnectionEvent::Dispatch(DispatchEvent::Resumed),
                    &connecting,
                );
                let status = shared.clone();
                tokio::spawn(async move { status.send_status(false).await });
                let subscriptions = shared.clone();
                tokio::spawn(async move { subscriptions.resubscribe(false).await });
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
