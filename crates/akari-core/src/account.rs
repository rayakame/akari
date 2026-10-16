use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::cache::{CacheThread, DiskCache, Flusher};
use crate::gateway::session::Timing;
use crate::gateway::{
    ConnectionEvent, DispatchEvent, Gateway, GatewayCommand, GatewayError, GuildSubscription,
    MemberLists, PresenceStatus, SendError,
};
use crate::model::{ChannelId, GuildId, MessageId, UserId};
use crate::rest::{AccountRest, CreateMessage, Page, Query, RequestError};
use crate::state::{
    Ack, ConnectionState, Cursor, LoadKind, LoadTicket, Message, Store, WindowLimits,
    message_length,
};
use crate::{DiscordClient, LoadReport, MessagePreparer, Token, ms};
use acks::{AckSender, Pace, Refusal};

const REFRESH_LIMIT: u8 = 100;
const JUMP_LIMIT: u8 = 50;
const REFRESH_RETRIES: u32 = 3;
// Short in tests, so retries don't slow the suite down.
const SERVER_ERROR_RETRY: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};
// A failed ack's first retry, short in tests too; later ones wait longer.
const ACK_RETRY: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(2)
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
    /// The newest messages: fills a window, refreshes a stale one, or jumps to the present; not
    /// when it would replace the window for a later-placed `Around` (`PageOutcome::Overtaken`).
    Latest { limit: u8 },
    /// Before the window's first message.
    Older { limit: u8 },
    /// After the window's last message, towards the present.
    Newer { limit: u8 },
    /// Replaces the window with the messages around `id`, unless a newer `Latest` or `Around`
    /// load, or [`Account::supersede_around_loads`], comes before the page.
    Around { id: MessageId, limit: u8 },
}

fn kind_name(kind: LoadKind) -> &'static str {
    match kind {
        LoadKind::Latest => "latest",
        LoadKind::Refresh => "refresh",
        LoadKind::Older => "older",
        LoadKind::Newer => "newer",
        LoadKind::Around(_) => "around",
    }
}

fn log_page(channel: ChannelId, kind: LoadKind, report: &LoadReport) {
    tracing::debug!(
        target: "akari_core::load",
        channel_id = channel.get(),
        kind = kind_name(kind),
        queued_ms = ms(report.queued),
        limited_ms = ms(report.limited),
        headers_ms = ms(report.headers),
        new_connection = report.new_connection,
        body_ms = ms(report.body),
        bytes = report.bytes,
        decode_ms = ms(report.decode),
        store_ms = ms(report.store),
        received = report.received,
        retries = report.retries,
        remaining = report.remaining,
        reset_after_ms = report.reset_after.map(ms),
        prepare_ms = ms(report.prepare),
        prepared = report.prepared,
        "history page loaded"
    );
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
    rest: Arc<AccountRest>,
    acks: AckSender,
    nonces: Nonces,
    status: Mutex<Status>,
    // What op 37 last asked for per guild in this session.
    subscribed: Mutex<BTreeMap<GuildId, GuildSubscription>>,
    member_lists: AtomicBool,
    runtime: tokio::runtime::Handle,
    delivering: Mutex<BTreeSet<MessageId>>,
    flusher: Option<Flusher>,
    // The cache's thread and whose file it has open.
    cache: Option<(CacheThread, UserId)>,
    preparer: RwLock<Option<Arc<dyn MessagePreparer>>>,
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

// One delivery per pending message at a time: a second caller is refused instead of posting
// the message again and settling it twice.
struct Delivering<'a> {
    delivering: &'a Mutex<BTreeSet<MessageId>>,
    pending: MessageId,
}

impl Drop for Delivering<'_> {
    fn drop(&mut self) {
        self.delivering
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.pending);
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
    fn claim(&self, pending: MessageId) -> Result<Delivering<'_>, RequestError> {
        let claimed = self
            .delivering
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pending);
        if !claimed {
            return Err(RequestError::InvalidRequest);
        }
        Ok(Delivering {
            delivering: &self.delivering,
            pending,
        })
    }

    async fn load(
        self: &Arc<Self>,
        channel: ChannelId,
        kind: LoadKind,
        limit: u8,
        placing: Option<u64>,
    ) -> Result<Option<LoadReport>, RequestError> {
        let Some(ticket) = self.store.begin_load(channel, kind, placing) else {
            return Ok(None);
        };
        let started = Instant::now();
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
        let request = async {
            let mut attempt = 0;
            loop {
                match self.rest.list_messages(channel, query, limit).await {
                    Err(err) if attempt < retries && err.is_transient() => {
                        tokio::time::sleep(SERVER_ERROR_RETRY * 2u32.pow(attempt)).await;
                        attempt += 1;
                    }
                    loaded => break loaded,
                }
            }
        };
        let mut prepared = (Duration::ZERO, 0);
        let loaded = match self.cache_for(channel, ticket) {
            None => request.await,
            // A page never waits for the disk; cached messages show if they come first, or
            // when the page failed, e.g. offline.
            Some(cache) => {
                let fill = async {
                    if let Some(messages) = cache.read_window(channel).await
                        && !messages.is_empty()
                    {
                        self.prepare(messages.clone(), &mut prepared).await;
                        self.store.fill_from_cache(ticket, messages);
                    }
                };
                let mut request = std::pin::pin!(request);
                let mut fill = std::pin::pin!(fill);
                tokio::select! {
                    loaded = &mut request => {
                        if loaded.is_err() {
                            fill.await;
                        }
                        loaded
                    }
                    () = &mut fill => request.await,
                }
            }
        };
        match loaded {
            Ok(Page {
                messages,
                received,
                mut report,
            }) => {
                self.prepare_page(&messages, &mut prepared).await;
                unfinished.ticket = None;
                let reached_end = received < usize::from(limit);
                let storing = Instant::now();
                {
                    let _gate = self.acks.gate();
                    let (outcome, ack) = self.store.finish_load(ticket, messages, reached_end);
                    report.outcome = outcome;
                    report.store = storing.elapsed();
                    self.ack_reading(ack);
                }
                (report.prepare, report.prepared) = prepared;
                log_page(channel, ticket.kind, &report);
                Ok(Some(report))
            }
            Err(err) => {
                unfinished.ticket = None;
                self.store.abort_load(ticket);
                self.failed(&err);
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = channel.get(),
                    kind = kind_name(ticket.kind),
                    after_ms = ms(started.elapsed()),
                    error = err.kind(),
                    "history page failed"
                );
                Err(err)
            }
        }
    }

    // On a blocking thread, so a slow preparer holds up only this load.
    async fn prepare(&self, messages: Vec<Arc<Message>>, totals: &mut (Duration, u32)) {
        let Some(preparer) = self
            .preparer
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return;
        };
        let started = Instant::now();
        let prepared = tokio::task::spawn_blocking(move || preparer.prepare(&messages)).await;
        totals.0 += started.elapsed();
        match prepared {
            Ok(count) => totals.1 += count,
            Err(_) => tracing::warn!("the message preparer failed; the page loads unprepared"),
        }
    }

    // The store converts the page under its lock; the preparer gets its own copy outside it.
    async fn prepare_page(&self, page: &[crate::model::Message], totals: &mut (Duration, u32)) {
        if self
            .preparer
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            return;
        }
        let messages = page
            .iter()
            .map(|message| {
                Arc::new(Message::from_wire(message.clone(), &mut |user| {
                    Arc::new(crate::state::User::from_wire(user))
                }))
            })
            .collect();
        self.prepare(messages, totals).await;
    }

    // Only the first page of an empty window can use the cache's copy of the channel.
    fn cache_for(&self, channel: ChannelId, ticket: LoadTicket) -> Option<&CacheThread> {
        let (cache, owner) = self.cache.as_ref()?;
        if ticket.kind != LoadKind::Latest {
            return None;
        }
        // READY named another account than the host: that file isn't this session's.
        if self
            .store
            .current_user()
            .is_some_and(|current| current.user.id != *owner)
        {
            return None;
        }
        let empty = self
            .store
            .messages(channel)
            .is_some_and(|window| window.messages.is_empty());
        empty.then_some(cache)
    }

    // A 401 means the token is gone, like the gateway's 4004: the UI has to log in again.
    fn failed(&self, err: &RequestError) {
        if matches!(err, RequestError::Unauthorized) {
            let error = Some(Arc::new(GatewayError::AuthenticationFailed));
            self.store.set_connection(ConnectionState::Closed { error });
            self.store.stop_reading();
            self.gateway.close();
        }
    }

    fn ack_refused(&self, refusal: Refusal) {
        let channel_id = refusal.channel.get();
        let message_id = refusal.sent.get();
        if self
            .store
            .undo_ack(refusal.channel, refusal.undo, refusal.after)
        {
            tracing::warn!(
                target: "akari_core::acks",
                channel_id,
                message_id,
                "refused ack undone"
            );
        } else {
            tracing::debug!(
                target: "akari_core::acks",
                channel_id,
                message_id,
                "refused ack left: the channel's read state changed since"
            );
        }
    }

    // While the user follows a channel, its acks go out paced, like the official client's.
    fn ack_reading(&self, ack: Option<Ack>) {
        if let Some(ack) = ack {
            self.acks.want(ack, Pace::Paced);
        }
    }

    // Without the gate: the pump forgets and swaps itself, so its own acks can't come between.
    fn dispatched(&self, event: ConnectionEvent, connecting: &dyn Fn() -> bool) {
        if let ConnectionEvent::Dispatch(DispatchEvent::MessageAck(ack)) = &event
            && ack.manual
            && ack.ack_type == 0
        {
            self.acks.forget(ack.channel_id);
        }
        let ack = on_event(&self.store, event, connecting);
        self.ack_reading(ack);
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
        self.rest.usable()?;
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
        Ok(Queued { pending, detached })
    }

    // Like the official client, a detached window jumps to the present; a stale one waits for
    // its refresh. The send doesn't wait for the jump.
    async fn deliver_queued(
        self: &Arc<Self>,
        channel: ChannelId,
        pending: MessageId,
        content: String,
        detached: bool,
    ) -> Result<MessageId, RequestError> {
        if detached {
            let (jumped, delivered) = tokio::join!(
                self.load(channel, LoadKind::Latest, JUMP_LIMIT, None),
                self.deliver(channel, pending, content),
            );
            if let Err(err) = jumped {
                tracing::debug!(error = %err, "couldn't jump to the present before sending");
            }
            return delivered;
        }
        self.deliver(channel, pending, content).await
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
            let refreshed = self
                .load(channel, LoadKind::Refresh, REFRESH_LIMIT, None)
                .await;
            if refreshed.is_err() && self.rest.is_unauthorized() {
                return;
            }
        }
    }

    fn close(&self) {
        self.gateway.close();
        self.rest.close();
        self.acks.close();
        if let Some(flusher) = &self.flusher {
            if self.expired() {
                flusher.delete();
            } else {
                flusher.close();
            }
        }
    }

    // Discord ended the session: the host logs in again, and the cache goes with the token.
    fn expired(&self) -> bool {
        matches!(
            self.store.connection(),
            ConnectionState::Closed { error: Some(error) }
                if matches!(*error, GatewayError::AuthenticationFailed)
        )
    }
}

impl Account {
    #[cfg(test)]
    pub(crate) fn start(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
    ) -> Result<Self, GatewayError> {
        Self::start_with_cache(client, token, timing, limits, None)
    }

    pub(crate) fn start_with_cache(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
        cache: Option<DiskCache>,
    ) -> Result<Self, GatewayError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| GatewayError::NoRuntime)?;
        let rest = Arc::new(AccountRest::new(
            client.clone(),
            token.clone(),
            SERVER_ERROR_RETRY,
        ));
        let store = Store::new(limits);
        let thread = cache
            .as_ref()
            .map(|cache| (cache.thread.clone(), cache.user));
        if let Some((thread, owner)) = thread.clone() {
            let store = store.clone();
            runtime.spawn(async move {
                let Some(cached) = thread.read_entities().await else {
                    return;
                };
                match &cached.current_user {
                    Some(current) if current.user.id == owner => {
                        store.restore(cached);
                    }
                    Some(_) => {
                        tracing::info!(target: "akari_core::cache", reason = "other_user", "cache not used");
                    }
                    None => {}
                }
            });
        }
        let flusher = cache.map(|cache| Flusher::start(&runtime, store.clone(), cache));
        let gateway = Gateway::start(client, token, timing)?;
        let shared = Arc::new_cyclic(|weak: &Weak<Shared>| {
            let unauthorized = {
                let weak = weak.clone();
                move || {
                    if let Some(shared) = weak.upgrade() {
                        shared.failed(&RequestError::Unauthorized);
                    }
                }
            };
            let refused = {
                let weak = weak.clone();
                move |refusal| {
                    if let Some(shared) = weak.upgrade() {
                        shared.ack_refused(refusal);
                    }
                }
            };
            Shared {
                gateway,
                store,
                acks: AckSender::start(&runtime, rest.clone(), ACK_RETRY, unauthorized, refused),
                rest,
                nonces: Nonces::default(),
                status: Mutex::default(),
                subscribed: Mutex::default(),
                member_lists: AtomicBool::new(true),
                delivering: Mutex::default(),
                runtime: runtime.clone(),
                flusher,
                cache: thread,
                preparer: RwLock::new(None),
            }
        });
        // Built before the task starts, so it runs even if the task is never polled.
        let finish = Finish::new(shared.clone());
        runtime.spawn(pump(finish));
        runtime.spawn(mutes::end_mutes(
            shared.store.clone(),
            shared.store.subscribe(),
        ));
        Ok(Self { shared })
    }

    /// Runs `preparer` on every history page and disk cache fill before its messages enter
    /// a window. Replaces an earlier one.
    pub fn set_message_preparer(&self, preparer: Arc<dyn MessagePreparer>) {
        *self
            .shared
            .preparer
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(preparer);
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
        self.shared.store.stop_reading();
        if let Some(flusher) = &self.shared.flusher {
            flusher.flush_soon();
        }
    }

    /// Ends the session. Subscriptions end once the connection is closed. The disk cache gets
    /// the state as of this call; nothing that changes later is written.
    pub fn close(&self) {
        self.shared.close();
    }

    /// Writes what changed to the disk cache now, e.g. before the app quits. Resolves when
    /// it's written, or at once without a cache.
    pub async fn flush_cache(&self) {
        if let Some(flusher) = &self.shared.flusher {
            flusher.flush().await;
        }
    }

    /// Marks the channel as viewed: the store keeps its messages and adds new ones from
    /// the gateway. Past 10 viewed channels, the least recently viewed one without pending or
    /// failed messages loses its messages.
    pub fn view_channel(&self, channel: ChannelId) {
        self.shared.view(channel);
    }

    /// The channel the user is reading at its newest messages, or `None`. While set, the
    /// channel is acked up to the newest message its list shows at the present, and again as
    /// that list grows, paced. Hosts set it only while the user is engaged with the channel at
    /// the bottom of its list. A dropped connection clears it: set it again once online and the
    /// list is refreshed.
    pub fn set_reading(&self, channel: Option<ChannelId>) {
        let _gate = self.shared.acks.gate();
        let ack = self.shared.store.set_reading(channel);
        self.shared.ack_reading(ack);
    }

    /// Marks the channel read now (Escape, the context menu).
    pub fn mark_read(&self, channel: ChannelId) {
        let _gate = self.shared.acks.gate();
        if let Some(ack) = self.shared.store.mark_read(channel) {
            self.shared.acks.want(ack, Pace::Explicit);
        }
    }

    /// Marks these guilds read now with one bulk ack (Shift+Escape, a server's or a folder's
    /// context menu).
    pub fn mark_guilds_read(&self, guilds: &[GuildId]) {
        let _gate = self.shared.acks.gate();
        let acks = self.shared.store.mark_guilds_read(guilds);
        if !acks.is_empty() {
            self.shared.acks.want_bulk(acks);
        }
    }

    /// Loads messages into the channel's window within Discord's rate limits. `Latest` and
    /// `Around` view the channel first; `Older` and `Newer` continue a window and do nothing
    /// without one. A 401 closes the account like a rejected token.
    /// `None` when there was nothing to load.
    pub async fn load_messages(
        &self,
        channel: ChannelId,
        load: MessageLoad,
    ) -> Result<Option<LoadReport>, RequestError> {
        self.load_placed(channel, load, None).await
    }

    /// Like [`Account::load_messages`], in the place `placing` holds among the channel's
    /// `Latest` and `Around` loads and supersedes, wherever the load begins.
    pub async fn load_messages_placed(
        &self,
        channel: ChannelId,
        load: MessageLoad,
        placing: u64,
    ) -> Result<Option<LoadReport>, RequestError> {
        self.load_placed(channel, load, Some(placing)).await
    }

    /// The next place among loads and supersedes, for [`Account::load_messages_placed`]. A host
    /// takes it where the user acts, so a later supersede overtakes the load even if it comes
    /// before the load begins.
    pub fn reserve_placing(&self) -> u64 {
        self.shared.store.next_placing()
    }

    async fn load_placed(
        &self,
        channel: ChannelId,
        load: MessageLoad,
        placing: Option<u64>,
    ) -> Result<Option<LoadReport>, RequestError> {
        let (kind, limit) = load.parts();
        let loaded = self.shared.load(channel, kind, limit, placing).await;
        // Cached messages left stale hold live ones back; before READY its refresh takes them.
        if loaded.is_err()
            && kind == LoadKind::Latest
            && self.shared.store.had_ready()
            && self.shared.store.stale_channels().contains(&channel)
        {
            let shared = self.shared.clone();
            self.shared.runtime.spawn(async move {
                let _ = shared
                    .load(channel, LoadKind::Refresh, REFRESH_LIMIT, None)
                    .await;
            });
        }
        loaded
    }

    /// Drops the pages of `Around` loads running for the channel, so they don't replace its
    /// window: Escape or a mark as read during a jump, where no `Latest` load overtakes them.
    pub fn supersede_around_loads(&self, channel: ChannelId) {
        self.shared.store.supersede_around(channel);
    }

    /// Shows the message as pending at once and sends it. Resolves with Discord's ID; on an
    /// error the pending message is marked failed and stays until retried or discarded.
    pub async fn send_message(
        &self,
        channel: ChannelId,
        content: String,
    ) -> Result<MessageId, RequestError> {
        let Queued { pending, detached } = self.shared.queue(channel, &content)?;
        let _delivering = self.shared.claim(pending)?;
        self.shared
            .deliver_queued(channel, pending, content, detached)
            .await
    }

    /// The first half of [`Account::send_message`]: shows the message as pending and returns
    /// its provisional ID, or refuses (empty or too long content, a closed account) before
    /// anything is queued. [`Account::deliver_message`] sends it.
    pub fn queue_message(
        &self,
        channel: ChannelId,
        content: &str,
    ) -> Result<MessageId, RequestError> {
        Ok(self.shared.queue(channel, content)?.pending)
    }

    /// Sends a message [`Account::queue_message`] queued, like [`Account::send_message`].
    /// Fails with [`RequestError::InvalidRequest`] if it isn't pending or is already being
    /// delivered.
    pub async fn deliver_message(
        &self,
        channel: ChannelId,
        pending: MessageId,
    ) -> Result<MessageId, RequestError> {
        let shared = &self.shared;
        let _delivering = shared.claim(pending)?;
        let message = shared
            .store
            .pending_message(channel, pending)
            .ok_or(RequestError::InvalidRequest)?;
        let detached = shared
            .store
            .messages(channel)
            .is_some_and(|window| !window.latest);
        shared
            .deliver_queued(channel, pending, message.content.to_string(), detached)
            .await
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
        let _delivering = shared.claim(pending)?;
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
        let _delivering = self.shared.claim(pending)?;
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
        let error = self
            .ended
            .take()
            .unwrap_or_else(|| Some(Arc::new(GatewayError::Stopped)));
        self.shared
            .store
            .set_connection(ConnectionState::Closed { error });
        self.shared.store.stop_reading();
        // close() does nothing once the gateway has ended, so it runs on every path. After the
        // state, so it sees why the session ended.
        self.shared.close();
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
                // Before the conversion, whose local acks the swap keeps: those are the user's
                // now, not ones READY may have overtaken.
                let preparing = {
                    let _gate = shared.acks.gate();
                    shared.acks.forget_all();
                    shared.store.begin_ready()
                };
                let store = shared.store.clone();
                let prepared =
                    tokio::task::spawn_blocking(move || store.prepare(preparing, *ready)).await;
                match prepared {
                    Ok(next) => {
                        let replaced = {
                            let _gate = shared.acks.gate();
                            let replaced = shared.store.replace(next);
                            shared
                                .acks
                                .reconcile(|channel| replaced.read_states.get(&channel).copied());
                            replaced
                        };
                        drop(replaced);
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
                shared.dispatched(
                    ConnectionEvent::Dispatch(DispatchEvent::Resumed),
                    &connecting,
                );
                let status = shared.clone();
                tokio::spawn(async move { status.send_status(false).await });
                let subscriptions = shared.clone();
                tokio::spawn(async move { subscriptions.resubscribe().await });
            }
            Ok(event) => shared.dispatched(event, &connecting),
            Err(GatewayError::Closed) => break None,
            Err(error) => break Some(Arc::new(error)),
        }
    };
    finish.ended = Some(error);
}

// Queued events mustn't make a disconnected account look connected. disconnect() idles the
// gateway before it takes the store's lock, so `connecting` checked under that lock sees it.
fn on_event(store: &Store, event: ConnectionEvent, connecting: &dyn Fn() -> bool) -> Option<Ack> {
    match event {
        ConnectionEvent::Dispatch(event) => {
            let ready = matches!(event, DispatchEvent::Ready(_) | DispatchEvent::Resumed);
            let ack = store.apply(event);
            if ready {
                store.set_connection_if(ConnectionState::Online, connecting);
            }
            ack
        }
        ConnectionEvent::Reconnecting { .. } => {
            store.stop_reading();
            store.set_connection_if(ConnectionState::Connecting, connecting);
            None
        }
        #[cfg(feature = "capture")]
        ConnectionEvent::CapturedReady(_) => None,
    }
}

mod acks;
mod mutes;

#[cfg(test)]
mod tests;
