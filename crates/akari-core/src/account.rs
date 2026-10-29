use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::cache::{CacheThread, DiskCache, Flusher};
use crate::error::TransportErrorKind;
use crate::gateway::session::Timing;
use crate::gateway::{
    ConnectionEvent, DispatchEvent, Gateway, GatewayCommand, GatewayError, GuildSubscription,
    MemberLists, PresenceStatus, SendError,
};
use crate::media::{ImageError, ImageFile, ImageKey, ImageRequest, Images};
use crate::model::{ChannelId, ChannelType, GuildId, MessageId, MessageType, Timestamp, UserId};
use crate::rest::{
    AccountRest, CreateMessage, CreateThread, Failure, Page, Query, RequestError, uri_component,
};
use crate::state::{
    Ack, ConnectionState, Cursor, ForumView, Learned, LoadKind, LoadTicket, Message, OLDER_PAGE,
    PageKey, StarterFetch, Store, WindowLimits, message_length,
};
use crate::{DiscordClient, LoadReport, MessagePreparer, Token, ms};
use acks::{AckSender, Pace, Refusal};

const REFRESH_LIMIT: u8 = 100;
// Discord's limit for a thread's name, in code points, and the archive time its client picks
// when the parent names none.
const THREAD_NAME_LIMIT: usize = 100;
const DEFAULT_ARCHIVE_MINUTES: u32 = 4320;
const MAX_ACTIVE_THREADS: u32 = 160_006;
const MAX_ANNOUNCEMENT_THREADS: u32 = 160_007;
const AUTOMOD_BLOCKED: u32 = 200_000;
const AUTOMOD_BLOCKED_NAME: u32 = 200_001;
const INVALID_FORM_BODY: u32 = 50035;
const SLOWMODE: u32 = 20016;
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
    /// Replaces the window with the messages around `id`, unless a later-placed `Latest` or
    /// `Around` load, or [`Account::supersede_around_loads`], comes first.
    Around { id: MessageId, limit: u8 },
}

/// Where the user chose to join or leave a thread, which Discord's own client tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThreadLocation {
    /// The "…" menu of a thread beside its parent.
    SidebarOverflow,
    /// The "…" menu of a thread open full width.
    ToolbarOverflow,
    /// A thread row's context menu.
    ContextMenu,
}

impl ThreadLocation {
    fn query(self) -> &'static str {
        match self {
            Self::SidebarOverflow => "Sidebar%20Overflow",
            Self::ToolbarOverflow => "Toolbar%20Overflow",
            Self::ContextMenu => "Context%20Menu",
        }
    }
}

// A failure after the request left: Discord may have created the thread behind it.
fn maybe_created(err: &RequestError) -> bool {
    match err {
        RequestError::ServerError { .. } => true,
        RequestError::Network(err) => !matches!(
            err.kind(),
            TransportErrorKind::Connect | TransportErrorKind::Tls
        ),
        _ => false,
    }
}

/// Where the user started creating a thread, which Discord's own client tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CreateThreadLocation {
    /// A message's context menu.
    Message,
    /// The Threads browser's Create button.
    ThreadBrowserToolbar,
    /// The Threads browser's "Create Thread" when it lists none.
    ThreadBrowserEmptyState,
    /// The shortcut for creating a thread.
    KeyboardShortcut,
}

impl CreateThreadLocation {
    fn words(self) -> &'static str {
        match self {
            Self::Message => "Message",
            Self::ThreadBrowserToolbar => "Thread Browser Toolbar",
            Self::ThreadBrowserEmptyState => "Thread Browser Empty State",
            Self::KeyboardShortcut => "Keyboard Shortcut",
        }
    }
}

/// Why [`Account::create_thread`] created nothing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CreateThreadError {
    /// The parent's slowmode for new threads runs; [`Store::thread_slowmode`] says until when.
    #[error("slowmode: creating threads here has to wait")]
    Slowmode,
    /// The server has as many active threads as it may, or announcement threads.
    #[error("the server has the most active threads it may")]
    TooManyThreads { announcement: bool },
    /// AutoMod refused the name (200001) or the thread (200000); `reason` is Discord's message.
    #[error("AutoMod refused the name")]
    NameBlocked { reason: String },
    /// Discord wants a name.
    #[error("Discord wants a name")]
    NameRequired,
    /// The request went out but its answer was lost or didn't read as the thread: it may exist
    /// already. The parent's slowmode for new threads starts as if it did.
    #[error("the thread may have been created")]
    MaybeCreated,
    /// Anything else, also a refusal before asking (`InvalidRequest`) and a captcha.
    #[error(transparent)]
    Request(#[from] RequestError),
}

// A page of archived threads asked for and not settled: dropping the request gives it up.
struct PendingPage<'a> {
    store: &'a Store,
    parent: ChannelId,
    page: Option<crate::state::OlderPage>,
}

impl Drop for PendingPage<'_> {
    fn drop(&mut self) {
        if let Some(page) = self.page.take() {
            self.store.abandon_older_threads(self.parent, page);
        }
    }
}

struct PendingPosts<'a> {
    store: &'a Store,
    key: &'a PageKey,
    page: Option<crate::state::OlderPage>,
}

impl Drop for PendingPosts<'_> {
    fn drop(&mut self) {
        if let Some(page) = self.page.take() {
            self.store.abandon_older_posts(self.key, page);
        }
    }
}

// Module 472456: the tags comma-joined in one value, the matching always.
fn tag_query(key: &PageKey) -> String {
    let mut query = String::new();
    if !key.tags.is_empty() {
        let tags: Vec<String> = key.tags.iter().map(|tag| tag.get().to_string()).collect();
        query.push_str("&tag=");
        query.push_str(&uri_component(&tags.join(",")));
    }
    query.push_str(if key.match_all {
        "&tag_setting=match_all"
    } else {
        "&tag_setting=match_some"
    });
    query
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
    images: Arc<Images>,
    lookup_timeout: Duration,
    // Wakes the post-data task when cards want their posts' first messages.
    previews: Arc<previews::Wake>,
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
                    let (outcome, acks) = self.store.finish_load(ticket, messages, reached_end);
                    report.outcome = outcome;
                    report.store = storing.elapsed();
                    self.ack_reading(acks);
                }
                (report.prepare, report.prepared) = prepared;
                log_page(channel, ticket.kind, &report);
                self.fetch_starter(channel);
                Ok(Some(report))
            }
            Err(err) => {
                unfinished.ticket = None;
                self.store.abort_load(ticket);
                self.store.fail_starter(channel);
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

    // Discord can send a thread's first message without the parent's message it started from.
    // A user account can't get one message, so it asks for the page of one around it.
    fn fetch_starter(self: &Arc<Self>, thread: ChannelId) {
        let Some((channel, id)) = self.store.starter_to_fetch(thread) else {
            return;
        };
        let shared = self.clone();
        self.runtime.spawn(async move {
            let fetch = match shared
                .rest
                .list_messages(channel, Query::Around(id), 1)
                .await
            {
                Ok(page) => match page.messages.into_iter().find(|message| message.id == id) {
                    Some(message) => {
                        let mut prepared = (Duration::ZERO, 0);
                        shared
                            .prepare_page(std::slice::from_ref(&message), &mut prepared)
                            .await;
                        StarterFetch::Found(Box::new(message))
                    }
                    None => StarterFetch::Gone,
                },
                // Deleted, or the user can't read the parent.
                Err(RequestError::Discord { .. }) => StarterFetch::Gone,
                Err(err) => {
                    shared.failed(&err);
                    StarterFetch::Failed
                }
            };
            tracing::debug!(
                target: "akari_core::load",
                thread_id = thread.get(),
                outcome = match fetch {
                    StarterFetch::Found(_) => "found",
                    StarterFetch::Gone => "gone",
                    StarterFetch::Failed => "failed",
                },
                "starter message fetched"
            );
            shared.store.settle_starter(thread, fetch);
        });
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
    fn ack_reading(&self, acks: Vec<Ack>) {
        for ack in acks {
            self.acks.want(ack, Pace::Paced);
        }
    }

    // Without the gate: the pump forgets and swaps itself, so its own acks can't come between.
    fn dispatched(self: &Arc<Self>, event: ConnectionEvent, connecting: &dyn Fn() -> bool) {
        if let ConnectionEvent::Dispatch(DispatchEvent::MessageAck(ack)) = &event
            && ack.manual
            && ack.ack_type == 0
        {
            self.acks.forget(ack.channel_id);
        }
        let starter = match &event {
            ConnectionEvent::Dispatch(DispatchEvent::MessageCreate(message))
                if message.kind == MessageType::ThreadStarterMessage =>
            {
                Some(message.channel_id)
            }
            _ => None,
        };
        let acks = on_event(&self.store, event, connecting);
        self.ack_reading(acks);
        if let Some(thread) = starter {
            self.fetch_starter(thread);
        }
    }

    async fn create_thread(
        &self,
        parent: ChannelId,
        message: Option<MessageId>,
        name: &str,
        private: bool,
        location: CreateThreadLocation,
    ) -> Result<ChannelId, CreateThreadError> {
        self.rest.usable()?;
        let name = name.trim();
        let length = message_length(name);
        let channel = self
            .store
            .channel(parent)
            .ok_or(RequestError::InvalidRequest)?;
        let allowed = self.store.thread_creation(parent);
        let public = if channel.kind == ChannelType::GuildNews {
            ChannelType::NewsThread
        } else {
            ChannelType::PublicThread
        };
        let kind = match (message, private) {
            (Some(message), false) if self.store.may_start_thread(parent, message) => public,
            (None, false) if allowed.public => public,
            (None, true) if allowed.private => ChannelType::PrivateThread,
            _ => return Err(RequestError::InvalidRequest.into()),
        };
        if length == 0 || length > THREAD_NAME_LIMIT {
            return Err(RequestError::InvalidRequest.into());
        }
        if self
            .store
            .thread_slowmode(parent)
            .is_some_and(|slowmode| slowmode.until.is_some())
        {
            return Err(CreateThreadError::Slowmode);
        }
        let body = CreateThread {
            name: name.to_owned(),
            kind: kind.into(),
            auto_archive_duration: channel
                .default_auto_archive_duration
                .unwrap_or(DEFAULT_ARCHIVE_MINUTES),
            location: location.words(),
        };
        let answer = self.rest.create_thread(parent, message, &body).await;
        let created = match answer {
            Ok(thread) => {
                self.store.start_thread_cooldown(parent);
                thread
                    .and_then(|thread| self.store.keep_created_thread(thread))
                    .ok_or(CreateThreadError::MaybeCreated)
            }
            Err(Failure::Slowmode(wait)) => Err(self.slowed(parent, wait)),
            Err(Failure::Api(api)) => Err(match api.code {
                SLOWMODE => self.slowed(parent, None),
                MAX_ACTIVE_THREADS => CreateThreadError::TooManyThreads {
                    announcement: false,
                },
                MAX_ANNOUNCEMENT_THREADS => {
                    CreateThreadError::TooManyThreads { announcement: true }
                }
                // The official client shows nothing for 200000; its reason belongs with the name.
                AUTOMOD_BLOCKED | AUTOMOD_BLOCKED_NAME => CreateThreadError::NameBlocked {
                    reason: api.message,
                },
                INVALID_FORM_BODY if api.field_errors.iter().any(|error| error.path == "name") => {
                    CreateThreadError::NameRequired
                }
                _ => RequestError::from(Failure::Api(api)).into(),
            }),
            Err(other) => {
                let err = RequestError::from(other);
                self.failed(&err);
                if maybe_created(&err) {
                    self.store.start_thread_cooldown(parent);
                    Err(CreateThreadError::MaybeCreated)
                } else {
                    Err(err.into())
                }
            }
        };
        tracing::debug!(
            channel_id = parent.get(),
            from_message = message.is_some(),
            kind = u16::from(kind),
            location = location.words(),
            outcome = match &created {
                Ok(_) => "created",
                Err(CreateThreadError::Slowmode) => "slowmode",
                Err(CreateThreadError::TooManyThreads { .. }) => "too_many_threads",
                Err(CreateThreadError::NameBlocked { .. }) => "name_blocked",
                Err(CreateThreadError::NameRequired) => "name_required",
                Err(CreateThreadError::MaybeCreated) => "maybe_created",
                Err(CreateThreadError::Request(err)) => err.kind(),
            },
            "thread creation"
        );
        created
    }

    // Discord's slowmode answer: its wait, else the parent's whole interval. Without a slowmode
    // the store knows of, nothing would count down, so it's a plain rate limit.
    fn slowed(&self, parent: ChannelId, wait: Option<Duration>) -> CreateThreadError {
        let Some(slowmode) = self
            .store
            .thread_slowmode(parent)
            .filter(|slowmode| !slowmode.exempt)
        else {
            return RequestError::RateLimited { retry_after: wait }.into();
        };
        self.store
            .hold_thread_cooldown(parent, wait.unwrap_or(slowmode.interval));
        CreateThreadError::Slowmode
    }

    async fn thread_member(
        &self,
        thread: ChannelId,
        join: bool,
        location: ThreadLocation,
    ) -> Result<(), RequestError> {
        if !self.store.can_change_membership(thread) {
            return Err(RequestError::InvalidRequest);
        }
        let changed = self
            .rest
            .thread_member(thread, join, location.query())
            .await;
        tracing::debug!(
            thread_id = thread.get(),
            join,
            outcome = changed
                .as_ref()
                .map_or_else(RequestError::kind, |()| "done"),
            "thread membership"
        );
        if let Err(err) = &changed {
            self.failed(err);
        }
        changed?;
        let now = self
            .rest
            .server_now_millis()
            .map_or_else(|| self.store.now(), Timestamp::from_unix_millis);
        self.store.set_own_membership(thread, join.then_some(now));
        Ok(())
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
        self.previews.close();
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
        Self::start_with_cache(client, token, timing, limits, None, None)
    }

    // With `user`'s image directory.
    #[cfg(test)]
    pub(crate) fn start_as(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
        user: UserId,
    ) -> Result<Self, GatewayError> {
        Self::start_with_cache(client, token, timing, limits, None, Some(user))
    }

    pub(crate) fn start_with_cache(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
        cache: Option<DiskCache>,
        user: Option<UserId>,
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
        let images = Images::new(&client, user, store.clone(), rest.clone(), &runtime);
        let lookup_timeout = timing.lookup_timeout;
        let scan_timing = scans::ScanTiming {
            wait: timing.scan_wait,
            jitter: timing.scan_jitter,
        };
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
                images,
                lookup_timeout,
                previews: Arc::default(),
            }
        });
        // Built before the task starts, so it runs even if the task is never polled.
        let finish = Finish::new(shared.clone());
        runtime.spawn(pump(finish));
        runtime.spawn(deadlines::run_deadlines(
            shared.store.clone(),
            shared.store.subscribe(),
        ));
        runtime.spawn(scans::run_scans(
            shared.store.clone(),
            shared.rest.clone(),
            shared.store.subscribe(),
            scan_timing,
        ));
        let weak = Arc::downgrade(&shared);
        runtime.spawn(previews::run_post_data(
            shared.store.clone(),
            shared.rest.clone(),
            shared.previews.clone(),
            move || {
                if let Some(shared) = weak.upgrade() {
                    shared.failed(&RequestError::Unauthorized);
                }
            },
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

    /// The user agreed to see the guild's age-restricted channels, as the official client asks
    /// once per server. Kept with the account's cache.
    pub fn agree_to_age_restricted(&self, guild: GuildId) {
        self.shared.store.agree_to_age_restricted(guild);
    }

    /// The channels the user is reading at their newest messages, at most two (a channel and
    /// the thread beside it); further ones are ignored, and an empty slice reads none. Each is
    /// acked up to the newest message its list shows at the present, and again as that list
    /// grows, paced per channel; a forum, whose page shows, at "now" while it has new posts or
    /// posts with unread activity. Hosts name a channel only while the user is engaged with it at
    /// the bottom of its list. A dropped connection clears them: set them again once online and
    /// the lists are refreshed.
    pub fn set_reading(&self, channels: &[ChannelId]) {
        let _gate = self.shared.acks.gate();
        let acks = self.shared.store.set_reading(channels);
        self.shared.ack_reading(acks);
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

    /// Fetches a thread that a "started a thread" line in `parent` names when the store doesn't
    /// have it (archived, or never synced) and keeps it as Discord sends it: unjoined, unless
    /// the user is a member. Returns whether the store has the thread now. A 403 or 404, or a
    /// channel that isn't a thread, makes it [`Store::thread_unavailable`] for the session, and
    /// it isn't asked for again; other failures leave it to be fetched again.
    pub async fn fetch_thread(
        &self,
        parent: ChannelId,
        thread: ChannelId,
    ) -> Result<bool, RequestError> {
        if self.shared.store.channel(thread).is_some() {
            return Ok(true);
        }
        self.check_thread(parent, thread).await
    }

    /// Asks Discord about a thread the store may already have, e.g. one a host remembers from an
    /// earlier session: like [`Account::fetch_thread`], but it asks even then, and a 403 or 404
    /// also removes the thread from the store, as THREAD_DELETE would.
    pub async fn check_thread(
        &self,
        parent: ChannelId,
        thread: ChannelId,
    ) -> Result<bool, RequestError> {
        let shared = &self.shared;
        if shared.store.thread_unavailable(thread) {
            return Ok(false);
        }
        let learned = match shared.rest.get_channel(thread).await {
            Ok(channel) => Ok(shared.store.learn_thread(channel)),
            Err(RequestError::Discord {
                status: 403 | 404, ..
            }) => Ok(Learned::NotAThread),
            Err(err) => {
                shared.failed(&err);
                Err(err)
            }
        };
        if matches!(learned, Ok(Learned::NotAThread)) {
            shared.store.mark_thread_unavailable(parent, thread);
        }
        tracing::debug!(
            target: "akari_core::load",
            thread_id = thread.get(),
            outcome = match &learned {
                Ok(Learned::Kept) => "found",
                Ok(Learned::NotAThread) => "unavailable",
                Ok(Learned::Unplaced) => "no_guild",
                Err(err) => err.kind(),
            },
            "thread fetched"
        );
        learned.map(|learned| matches!(learned, Learned::Kept))
    }

    /// Loads the next page of `parent`'s archived threads into the store, as the official
    /// client's Threads browser does: 25 at a time, the newest message first, see
    /// [`Store::thread_browser`]. Nothing is asked while a page loads, after the last one, or
    /// after a failure or Discord's "not indexed yet" until the next connection.
    pub async fn load_older_threads(&self, parent: ChannelId) -> Result<(), RequestError> {
        let shared = &self.shared;
        let Some(page) = shared.store.begin_older_threads(parent) else {
            return Ok(());
        };
        let mut pending = PendingPage {
            store: &shared.store,
            parent,
            page: Some(page),
        };
        let query = format!(
            "archived=true&sort_by=last_message_time&sort_order=desc&limit={OLDER_PAGE}\
             &tag_setting=match_some&offset={}",
            page.offset
        );
        match shared.rest.search_threads(parent, &query).await {
            Ok(results) => {
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = parent.get(),
                    offset = page.offset,
                    threads = results.threads.as_ref().map(Vec::len),
                    more = results.has_more,
                    "archived threads loaded"
                );
                pending.page = None;
                shared.store.finish_older_threads(parent, page, results);
                Ok(())
            }
            Err(err) => {
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = parent.get(),
                    offset = page.offset,
                    outcome = err.kind(),
                    "archived threads failed"
                );
                shared.failed(&err);
                pending.page = None;
                shared.store.fail_older_threads(parent, page);
                Err(err)
            }
        }
    }

    /// The forum whose page shows, `None` once it closes. Opening one refreshes its list: while
    /// it stays open, who's listed and in what order change only with [`refresh_forum`], a new
    /// connection or the guild's thread sync, a deletion or a pin (G-1).
    ///
    /// [`refresh_forum`]: Self::refresh_forum
    pub fn open_forum(&self, forum: Option<ChannelId>) {
        let first = self.shared.store.open_forum(forum);
        // So the first cards show with their message.
        if let Some(forum) = forum {
            self.request_post_previews(forum, &first);
        }
    }

    /// Asks Discord for the first messages and owners of `posts`, whose cards show without
    /// them: the official client's post-data, 10 a request, one request at a time for the
    /// account. A post asked already, or that Discord didn't answer for in the last 30 s, isn't
    /// asked again. [`Store::post_preview`] has them; `PostPreviewChanged` says when.
    pub fn request_post_previews(&self, forum: ChannelId, posts: &[ChannelId]) {
        if self.shared.store.want_post_previews(forum, posts) {
            self.shared.previews.wake();
        }
    }

    /// The open forum's list as the store has it now: others' new posts join, the count starts
    /// over (the official client's "N New Posts" pill).
    pub fn refresh_forum(&self, forum: ChannelId) {
        self.shared.store.refresh_forum(forum);
    }

    /// Loads the next page of the forum's archived posts for `view`'s tags, as the official
    /// client's "Older Posts": 25 at a time, the newest message first whatever the sort, each tag
    /// filter paging on its own. Nothing is asked while a page loads, after the last one, or after
    /// a failure or Discord's "not indexed yet" until the next connection.
    pub async fn load_older_posts(
        &self,
        forum: ChannelId,
        view: &ForumView,
    ) -> Result<(), RequestError> {
        let shared = &self.shared;
        let key = PageKey::new(forum, view);
        let Some(page) = shared.store.begin_older_posts(&key) else {
            return Ok(());
        };
        let mut pending = PendingPosts {
            store: &shared.store,
            key: &key,
            page: Some(page),
        };
        let query = format!(
            "archived=true&sort_by=last_message_time&sort_order=desc&limit={OLDER_PAGE}{}&offset={}",
            tag_query(&key),
            page.offset
        );
        match shared.rest.search_threads(forum, &query).await {
            Ok(results) => {
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = forum.get(),
                    offset = page.offset,
                    tags = key.tags.len(),
                    posts = results.threads.as_ref().map(Vec::len),
                    more = results.has_more,
                    "archived posts loaded"
                );
                pending.page = None;
                shared.store.finish_older_posts(&key, page, results);
                Ok(())
            }
            Err(err) => {
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = forum.get(),
                    offset = page.offset,
                    outcome = err.kind(),
                    "archived posts failed"
                );
                shared.failed(&err);
                pending.page = None;
                shared.store.fail_older_posts(&key, page);
                Err(err)
            }
        }
    }

    /// Searches the forum's posts, active and archived, by name within `view`'s tags, as the
    /// official client does: Discord's first 25 matches in its order, kept in the store. `None`
    /// while Discord hasn't indexed the forum yet.
    pub async fn search_posts(
        &self,
        forum: ChannelId,
        name: &str,
        view: &ForumView,
    ) -> Result<Option<Vec<ChannelId>>, RequestError> {
        let shared = &self.shared;
        let asked = shared.store.generation();
        let key = PageKey::new(forum, view);
        let query = format!("name={}{}", uri_component(name), tag_query(&key));
        let results = match shared.rest.search_threads(forum, &query).await {
            Ok(results) => results,
            Err(err) => {
                shared.failed(&err);
                return Err(err);
            }
        };
        let found = shared.store.keep_found_threads(results, asked);
        tracing::debug!(
            target: "akari_core::load",
            channel_id = forum.get(),
            found = found.as_ref().map(Vec::len),
            "posts searched"
        );
        Ok(found)
    }

    /// The channel whose Threads browser is open, `None` once it closes. A new connection lets
    /// go of unjoined archived threads nothing shows (no window, box or "started a thread" line);
    /// the open browser's stay.
    pub fn set_browsing(&self, parent: Option<ChannelId>) {
        self.shared.store.set_browsing(parent);
    }

    /// Searches `parent`'s threads, active and archived, by name, as the official client does
    /// when the user presses Enter: Discord's first 25 matches in its order, kept in the store.
    /// `None` while Discord hasn't indexed the channel yet.
    pub async fn search_threads(
        &self,
        parent: ChannelId,
        name: &str,
    ) -> Result<Option<Vec<ChannelId>>, RequestError> {
        let shared = &self.shared;
        let asked = shared.store.generation();
        let query = format!("name={}&tag_setting=match_some", uri_component(name));
        let results = match shared.rest.search_threads(parent, &query).await {
            Ok(results) => results,
            Err(err) => {
                shared.failed(&err);
                return Err(err);
            }
        };
        let found = shared.store.keep_found_threads(results, asked);
        tracing::debug!(
            target: "akari_core::load",
            channel_id = parent.get(),
            found = found.as_ref().map(Vec::len),
            "threads searched"
        );
        Ok(found)
    }

    /// Joins the thread. The store has it joined once Discord says yes, without waiting for the
    /// gateway; a failure changes nothing. An archived thread, or one the store doesn't have, is
    /// refused with `InvalidRequest` before asking.
    pub async fn join_thread(
        &self,
        thread: ChannelId,
        location: ThreadLocation,
    ) -> Result<(), RequestError> {
        self.shared.thread_member(thread, true, location).await
    }

    /// Leaves the thread, as [`Account::join_thread`] joins it.
    pub async fn leave_thread(
        &self,
        thread: ChannelId,
        location: ThreadLocation,
    ) -> Result<(), RequestError> {
        self.shared.thread_member(thread, false, location).await
    }

    /// Creates a thread in `parent`, from `message` when given (never private then), as the
    /// official client does, and returns it: Discord's answer puts it in the store, and the
    /// parent's slowmode for new threads starts. Refused with `InvalidRequest` before asking: a
    /// name that is empty or longer than 100 code points once trimmed, a loaded message that
    /// can't start a thread ([`Store::can_start_thread`]), a kind the user may not create
    /// ([`Store::thread_creation`]); with `Slowmode` while that slowmode runs. The first message
    /// is an ordinary send in the thread afterwards.
    pub async fn create_thread(
        &self,
        parent: ChannelId,
        message: Option<MessageId>,
        name: &str,
        private: bool,
        location: CreateThreadLocation,
    ) -> Result<ChannelId, CreateThreadError> {
        self.shared
            .create_thread(parent, message, name, private, location)
            .await
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

    /// The image for `request`: a kept file with enough pixels, else downloaded from Discord's
    /// CDN or media proxy, never with the token. Waiters for the same image share a download;
    /// dropping the future stops waiting, and the last waiter to go cancels the download.
    /// Offline, only kept files answer.
    pub async fn image(&self, request: ImageRequest) -> Result<ImageFile, ImageError> {
        self.shared.images.image(request).await
    }

    /// A link to the original for Open in Browser and Copy Link; an attachment's link is
    /// renewed when it's about to expire.
    pub async fn original_url(&self, key: &ImageKey) -> Result<String, ImageError> {
        self.shared.images.original_url(key).await
    }

    /// Writes the original to `destination`, whatever its size; returns its bytes. Nothing is
    /// left at `destination` when it fails.
    pub async fn save_original(
        &self,
        key: &ImageKey,
        destination: &std::path::Path,
    ) -> Result<u64, ImageError> {
        self.shared.images.save_original(key, destination).await
    }

    /// Deletes the account's kept images.
    pub async fn clear_images(&self) {
        self.shared.images.clear().await;
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
fn on_event(store: &Store, event: ConnectionEvent, connecting: &dyn Fn() -> bool) -> Vec<Ack> {
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
            Vec::new()
        }
        #[cfg(feature = "capture")]
        ConnectionEvent::CapturedReady(_) => Vec::new(),
    }
}

mod acks;
mod deadlines;
mod expressions;
mod previews;
mod scans;

#[cfg(test)]
mod tests;
