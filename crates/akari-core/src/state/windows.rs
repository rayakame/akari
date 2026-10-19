use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use super::apply::wall_millis;
use super::events::StoreEvent;
use super::types::{Delivery, Message, ReferencedMessage, StarterMessage, User};
use crate::gateway::MessageUpdate;
use crate::load_report::PageOutcome;
use crate::model::{self, ChannelId, MessageId, MessageType, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowLimits {
    pub(crate) channels: usize,
    pub(crate) messages: usize,
}

pub(crate) const DEFAULT_LIMITS: WindowLimits = WindowLimits {
    channels: 10,
    messages: 200,
};

const RECENT_AUTHORS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Older,
    Newer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadKind {
    Latest,
    // Like Latest, but keeps the window's position if the page doesn't reach it.
    Refresh,
    Older,
    Newer,
    Around(MessageId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cursor {
    Latest,
    Before(MessageId),
    After(MessageId),
    Around(MessageId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoadTicket {
    pub(crate) channel: ChannelId,
    pub(crate) kind: LoadKind,
    pub(crate) cursor: Cursor,
    generation: u64,
    placing: u64,
    holds: bool,
    // The channel's newest message when the load began.
    newest: Option<MessageId>,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedWindow {
    // Milliseconds since the Unix epoch, so it orders against earlier sessions' windows.
    pub(crate) viewed_at: i64,
    pub(crate) messages: Vec<Arc<Message>>,
}

/// A snapshot of a viewed channel's loaded messages.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageWindow {
    /// Oldest first, without gaps.
    pub messages: Vec<Arc<Message>>,
    /// Ends with the channel's newest message; new messages are appended.
    pub latest: bool,
    /// Starts with the channel's first message.
    pub oldest: bool,
    /// A new session may have missed changes; a refresh is due.
    pub stale: bool,
    /// Our pending and failed messages, in send order, shown after `messages`.
    pub pending: Vec<Arc<Message>>,
}

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
struct Window {
    messages: Vec<Arc<Message>>,
    latest: bool,
    oldest: bool,
    stale: bool,
    // Loads that end at the present still running; live messages wait for them.
    holding: u32,
    // Live messages that arrived while stale or holding; appending them would leave a gap.
    held: Vec<Arc<Message>>,
    // Bumped whenever the messages are replaced, so a load for the old ones is dropped.
    generation: u64,
    // The newest placing, in the host's order, of a Latest or Around load or a supersede: an
    // Around page placed before it is dropped. Kept over a replace, as the host's order is.
    placing: u64,
    // The newest Around load's placing: a Latest page from before it leaves it the window.
    around_placing: u64,
    // At the present, a message up to this one that isn't among `messages` was deleted.
    present_through: Option<MessageId>,
    // The newest held message that was deleted: `present_through` once the held ones are rows.
    held_deleted: Option<MessageId>,
    // The newest held message dropped for the limit; rows before it can't join the held ones.
    held_dropped: Option<MessageId>,
    // Pending and failed messages; never trimmed.
    outbox: Vec<Arc<Message>>,
    loading: u32,
    // Edits and deletes while a load runs; its page may predate them.
    missed: Vec<Missed>,
    viewed_at: i64,
    // A page landed; until then the window holds at most live messages and isn't cached.
    filled: bool,
    // Filled by the disk cache; the first page checks only its own range, so older rows go.
    from_disk: bool,
}

#[cfg_attr(test, derive(PartialEq))]
enum Missed {
    Updated(Box<MessageUpdate>),
    Deleted(MessageId),
}

impl Window {
    fn appends_live(&self) -> bool {
        self.latest && !self.stale && self.holding == 0
    }

    fn holds_live(&self) -> bool {
        self.holding > 0 || (self.stale && self.latest)
    }

    fn forget_missed(&mut self) {
        if self.loading == 0 {
            self.missed.clear();
        }
    }

    fn release_or_drop_held(
        &mut self,
        channel_id: ChannelId,
        limit: usize,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.appends_live() {
            self.release_held(channel_id, limit, events);
        } else if !self.holds_live() {
            self.held.clear();
            self.held_deleted = None;
            self.held_dropped = None;
        }
    }

    fn merge(
        &mut self,
        channel_id: ChannelId,
        batch: Vec<Arc<Message>>,
        end: End,
        reached_end: bool,
        limit: usize,
        events: &mut Vec<StoreEvent>,
    ) {
        let mut added = Added::default();
        for message in batch {
            match Window::position(&self.messages, message.id) {
                Ok(index) => {
                    if self.messages[index] != message {
                        self.messages[index] = message.clone();
                        events.push(StoreEvent::MessageUpdated(message));
                    }
                }
                Err(index) => {
                    added.add(message.id);
                    self.messages.insert(index, message);
                }
            }
        }
        added.push(channel_id, events);
        match end {
            End::Older => self.oldest |= reached_end,
            End::Newer => self.latest |= reached_end,
        }
        let away_from_the_load = match end {
            End::Older => End::Newer,
            End::Newer => End::Older,
        };
        self.trim(channel_id, limit, away_from_the_load, events);
    }

    fn release_held(&mut self, channel_id: ChannelId, limit: usize, events: &mut Vec<StoreEvent>) {
        let newest = self.messages.last().map(|message| message.id);
        let held: Vec<_> = std::mem::take(&mut self.held)
            .into_iter()
            .filter(|message| newest.is_none_or(|newest| message.id > newest))
            .collect();
        let dropped = self
            .held_dropped
            .take()
            .filter(|dropped| newest < Some(*dropped));
        self.merge(channel_id, held, End::Newer, false, usize::MAX, events);
        // Appended live, the held messages would have trimmed every row before the dropped ones.
        if let Some(dropped) = dropped {
            self.drop_older(|id| id <= dropped, channel_id, events);
        }
        self.trim(channel_id, limit, End::Older, events);
        self.present_through = self.present_through.max(self.held_deleted.take());
    }

    // The page is the channel's newest; within its range it is the truth.
    fn reconcile(
        &mut self,
        channel_id: ChannelId,
        page: Vec<Arc<Message>>,
        reached_end: bool,
        limit: usize,
        events: &mut Vec<StoreEvent>,
    ) {
        if let (Some(first), Some(last)) = (page.first(), page.last()) {
            let (first, last) = (first.id, last.id);
            let ids: HashSet<MessageId> = page.iter().map(|message| message.id).collect();
            self.messages.retain(|message| {
                let gone = (first..=last).contains(&message.id) && !ids.contains(&message.id);
                if gone {
                    events.push(StoreEvent::MessageDeleted {
                        channel_id,
                        message_id: message.id,
                    });
                }
                !gone
            });
        }
        self.merge(
            channel_id,
            page,
            End::Older,
            reached_end,
            usize::MAX,
            events,
        );
        if self.holding == 0 {
            self.release_held(channel_id, usize::MAX, events);
        }
        self.stale = false;
        self.latest = true;
        self.trim(channel_id, limit, End::Older, events);
    }

    fn place(
        &mut self,
        channel_id: ChannelId,
        message: Arc<Message>,
        limit: usize,
        announce: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        match Window::position(&self.messages, message.id) {
            Ok(index) => {
                if self.messages[index] != message {
                    self.messages[index] = message.clone();
                    if announce {
                        events.push(StoreEvent::MessageUpdated(message));
                    }
                }
            }
            Err(_) if self.holds_live() => {
                upsert(&mut self.held, message);
                let excess = self.held.len().saturating_sub(limit);
                if let Some(dropped) = self.held.drain(..excess).next_back() {
                    self.held_dropped = self.held_dropped.max(Some(dropped.id));
                }
            }
            Err(index) if self.appends_live() => {
                self.messages.insert(index, message.clone());
                if announce {
                    events.push(StoreEvent::MessageInserted(message));
                }
                self.trim(channel_id, limit, End::Older, events);
            }
            Err(_) => {}
        }
    }

    fn drop_before(
        &mut self,
        first: Option<MessageId>,
        channel_id: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) {
        if let Some(first) = first {
            self.drop_older(|id| id < first, channel_id, events);
        }
    }

    fn drop_older(
        &mut self,
        older: impl Fn(MessageId) -> bool,
        channel_id: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) {
        let count = self
            .messages
            .iter()
            .take_while(|message| older(message.id))
            .count();
        if count == 0 {
            return;
        }
        self.messages.drain(..count);
        self.oldest = false;
        events.push(match self.range() {
            Some((first, last)) => StoreEvent::MessagesTrimmed {
                channel_id,
                first,
                last,
            },
            None => StoreEvent::MessagesCleared { channel_id },
        });
    }

    fn reaches(&self, page: &[Arc<Message>]) -> bool {
        match (page.first(), self.messages.last()) {
            (Some(first), Some(last)) => first.id <= last.id,
            _ => true,
        }
    }
    fn position(messages: &[Arc<Message>], id: MessageId) -> Result<usize, usize> {
        messages.binary_search_by_key(&id, |message| message.id)
    }

    fn range(&self) -> Option<(MessageId, MessageId)> {
        Some((self.messages.first()?.id, self.messages.last()?.id))
    }

    fn trim(
        &mut self,
        channel_id: ChannelId,
        limit: usize,
        end: End,
        events: &mut Vec<StoreEvent>,
    ) {
        let excess = self.messages.len().saturating_sub(limit);
        if excess == 0 {
            return;
        }
        match end {
            End::Older => {
                self.messages.drain(..excess);
                self.oldest = false;
            }
            End::Newer => {
                self.messages.truncate(limit);
                self.latest = false;
            }
        }
        if let Some((first, last)) = self.range() {
            events.push(StoreEvent::MessagesTrimmed {
                channel_id,
                first,
                last,
            });
        }
    }
}

#[derive(Default)]
struct Added(Option<(MessageId, MessageId)>);

impl Added {
    fn add(&mut self, id: MessageId) {
        self.0 = Some(
            self.0
                .map_or((id, id), |(first, last)| (first.min(id), last.max(id))),
        );
    }

    fn push(self, channel_id: ChannelId, events: &mut Vec<StoreEvent>) {
        if let Some((first, last)) = self.0 {
            events.push(StoreEvent::MessagesLoaded {
                channel_id,
                first,
                last,
            });
        }
    }
}

fn upsert(messages: &mut Vec<Arc<Message>>, message: Arc<Message>) {
    match Window::position(messages, message.id) {
        Ok(index) => messages[index] = message,
        Err(index) => messages.insert(index, message),
    }
}

#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct Windows {
    limits: WindowLimits,
    // Most recently viewed first.
    order: VecDeque<ChannelId>,
    windows: HashMap<ChannelId, Window>,
    generations: u64,
    // Threads whose first message came without the one it started from: what the fetch found.
    starters: HashMap<ChannelId, Starter>,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
enum Starter {
    Fetching,
    Found(Arc<Message>),
    Gone,
    // A request that failed; the thread's next page asks again.
    Failed,
}

/// What fetching a thread's starter found.
#[derive(Debug)]
pub(crate) enum StarterFetch {
    Found(Box<model::Message>),
    Gone,
    Failed,
}

impl Windows {
    pub(crate) fn new(limits: WindowLimits) -> Self {
        Self {
            limits,
            order: VecDeque::new(),
            windows: HashMap::new(),
            generations: 0,
            starters: HashMap::new(),
        }
    }

    // A thread started from a message begins with one pointing back at it.
    fn starter_row(&self, thread: ChannelId) -> Option<&Arc<Message>> {
        self.windows
            .get(&thread)?
            .messages
            .iter()
            .find(|message| message.kind == MessageType::ThreadStarterMessage)
    }

    fn origin(starter: &Message) -> Option<(ChannelId, MessageId)> {
        let reference = starter.reference.as_deref()?;
        Some((reference.channel_id?, reference.message_id?))
    }

    pub(crate) fn starter_message(&self, thread: ChannelId) -> Option<StarterMessage> {
        let row = self.starter_row(thread)?;
        let origin = Self::origin(row);
        // The parent's window has the newest copy, edits included.
        let fresh = |message: &Arc<Message>| {
            origin
                .and_then(|(channel, id)| self.message(channel, id))
                .unwrap_or_else(|| message.clone())
        };
        Some(match &row.referenced_message {
            ReferencedMessage::Message(message) => StarterMessage::Loaded(fresh(message)),
            ReferencedMessage::Deleted => StarterMessage::Unavailable,
            ReferencedMessage::NotIncluded if origin.is_none() => StarterMessage::Unavailable,
            ReferencedMessage::NotIncluded => match self.starters.get(&thread) {
                Some(Starter::Found(message)) => StarterMessage::Loaded(fresh(message)),
                Some(Starter::Gone | Starter::Failed) => StarterMessage::Unavailable,
                Some(Starter::Fetching) | None => StarterMessage::Loading,
            },
        })
    }

    pub(crate) fn wanted_starter(&self, thread: ChannelId) -> Option<(ChannelId, MessageId)> {
        let row = self.starter_row(thread)?;
        if row.referenced_message != ReferencedMessage::NotIncluded {
            return None;
        }
        match self.starters.get(&thread) {
            None | Some(Starter::Failed) => Self::origin(row),
            Some(Starter::Fetching | Starter::Found(_) | Starter::Gone) => None,
        }
    }

    // Marks the fetch as running, so each thread's starter is asked for once.
    pub(crate) fn starter_to_fetch(&mut self, thread: ChannelId) -> Option<(ChannelId, MessageId)> {
        let origin = self.wanted_starter(thread)?;
        self.starters.insert(thread, Starter::Fetching);
        Some(origin)
    }

    pub(crate) fn settle_starter(
        &mut self,
        thread: ChannelId,
        fetch: StarterFetch,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        // Dropped with its thread meanwhile.
        if !matches!(self.starters.get(&thread), Some(Starter::Fetching)) {
            return;
        }
        let settled = match fetch {
            StarterFetch::Found(message) => {
                Starter::Found(Arc::new(Message::from_wire(*message, &mut |user| {
                    intern(user, users, &[])
                })))
            }
            StarterFetch::Gone => Starter::Gone,
            StarterFetch::Failed => Starter::Failed,
        };
        self.starters.insert(thread, settled);
        if let Some(row) = self.starter_row(thread) {
            events.push(StoreEvent::MessageUpdated(row.clone()));
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.generations += 1;
        self.generations
    }

    pub(crate) fn view(&mut self, channel: ChannelId, events: &mut Vec<StoreEvent>) {
        let now = wall_millis();
        if let Some(index) = self.order.iter().position(|viewed| *viewed == channel) {
            self.order.remove(index);
            if let Some(window) = self.windows.get_mut(&channel) {
                window.viewed_at = now;
            }
        } else {
            let generation = self.next_generation();
            self.windows.insert(
                channel,
                Window {
                    latest: true,
                    generation,
                    viewed_at: now,
                    ..Window::default()
                },
            );
        }
        self.order.push_front(channel);
        while self.order.len() > self.limits.channels {
            // Windows with our own unsent messages stay, so those messages aren't lost.
            let Some(index) = self.order.iter().rposition(|viewed| {
                self.windows
                    .get(viewed)
                    .is_none_or(|window| window.outbox.is_empty())
            }) else {
                break;
            };
            if let Some(evicted) = self.order.remove(index) {
                self.windows.remove(&evicted);
                events.push(StoreEvent::MessagesCleared {
                    channel_id: evicted,
                });
            }
        }
    }

    // Only windows at the present that no new session made stale; others could be wrong.
    pub(crate) fn cache_windows(&self, per_window: usize) -> Vec<(ChannelId, CachedWindow)> {
        self.order
            .iter()
            .filter_map(|channel| {
                let window = self.windows.get(channel)?;
                if !window.filled || window.loading > 0 || !window.latest || window.stale {
                    return None;
                }
                let skip = window.messages.len().saturating_sub(per_window);
                Some((
                    *channel,
                    CachedWindow {
                        viewed_at: window.viewed_at,
                        messages: window.messages[skip..].to_vec(),
                    },
                ))
            })
            .collect()
    }

    pub(crate) fn snapshot(&self, channel: ChannelId) -> Option<MessageWindow> {
        let window = self.windows.get(&channel)?;
        Some(MessageWindow {
            messages: window.messages.clone(),
            latest: window.latest,
            oldest: window.oldest,
            stale: window.stale,
            pending: window.outbox.clone(),
        })
    }

    // The newest row of a list at the present, or a deleted message past it, which Discord keeps
    // as the channel's newest; `None` while the list is away from the present or stale.
    pub(crate) fn newest_at_present(&self, channel: ChannelId) -> Option<MessageId> {
        let window = self.windows.get(&channel)?;
        if !window.latest || window.stale {
            return None;
        }
        window
            .messages
            .last()
            .map(|message| message.id)
            .max(window.present_through)
    }

    pub(crate) fn message(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        let messages = &self.windows.get(&channel)?.messages;
        let index = Window::position(messages, id).ok()?;
        messages.get(index).cloned()
    }

    pub(crate) fn pending(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.windows
            .get(&channel)?
            .outbox
            .iter()
            .find(|message| message.id == id && message.delivery == Delivery::Pending)
            .cloned()
    }

    #[cfg(test)]
    pub(crate) fn held(&self, channel: ChannelId) -> Vec<Arc<Message>> {
        self.windows
            .get(&channel)
            .map(|window| window.held.clone())
            .unwrap_or_default()
    }

    pub(crate) fn live(
        &mut self,
        message: model::Message,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        let pending = message
            .nonce
            .as_ref()
            .and_then(model::Nonce::as_u64)
            .map(MessageId::new);
        self.receive(message, pending, users, events);
    }

    // The REST response for a pending message; the echo may have replaced it already.
    pub(crate) fn confirm(
        &mut self,
        channel: ChannelId,
        pending_id: MessageId,
        message: model::Message,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        if message.channel_id == channel {
            self.receive(message, Some(pending_id), users, events);
        }
    }

    fn receive(
        &mut self,
        message: model::Message,
        pending_id: Option<MessageId>,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        let channel_id = message.channel_id;
        let limit = self.limits.messages;
        let Some(window) = self.windows.get(&channel_id) else {
            return;
        };
        // Other users see our nonces and may send the same value.
        let pending = pending_id.and_then(|id| {
            window
                .outbox
                .iter()
                .position(|queued| queued.id == id && queued.author.id == message.author.id)
                .map(|index| (id, index))
        });
        let visible = Window::position(&window.messages, message.id).is_ok();
        if pending.is_none() && !visible && !window.appends_live() && !window.holds_live() {
            return;
        }
        let message = Arc::new(Message::from_wire(message, &mut |user| {
            intern(user, users, &window.messages)
        }));
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        match pending {
            Some((pending_id, index)) => {
                window.outbox.remove(index);
                window.place(channel_id, message.clone(), limit, false, events);
                events.push(StoreEvent::MessageReplaced {
                    channel_id,
                    pending_id,
                    message,
                });
            }
            None => window.place(channel_id, message, limit, true, events),
        }
    }

    pub(crate) fn queue(
        &mut self,
        channel: ChannelId,
        message: Arc<Message>,
        events: &mut Vec<StoreEvent>,
    ) {
        if let Some(window) = self.windows.get_mut(&channel) {
            window.outbox.push(message.clone());
            events.push(StoreEvent::MessageInserted(message));
        }
    }

    fn set_delivery(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        from: Delivery,
        to: Delivery,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Arc<Message>> {
        let window = self.windows.get_mut(&channel)?;
        let queued = window
            .outbox
            .iter_mut()
            .find(|queued| queued.id == id && queued.delivery == from)?;
        let mut next = (**queued).clone();
        next.delivery = to;
        *queued = Arc::new(next);
        events.push(StoreEvent::MessageUpdated(queued.clone()));
        Some(queued.clone())
    }

    pub(crate) fn fail(&mut self, channel: ChannelId, id: MessageId, events: &mut Vec<StoreEvent>) {
        self.set_delivery(channel, id, Delivery::Pending, Delivery::Failed, events);
    }

    pub(crate) fn retry(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Arc<Message>> {
        self.set_delivery(channel, id, Delivery::Failed, Delivery::Pending, events)
    }

    pub(crate) fn discard(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        let Some(window) = self.windows.get_mut(&channel) else {
            return false;
        };
        let Some(index) = window
            .outbox
            .iter()
            .position(|queued| queued.id == id && queued.delivery == Delivery::Failed)
        else {
            return false;
        };
        window.outbox.remove(index);
        events.push(StoreEvent::MessageDeleted {
            channel_id: channel,
            message_id: id,
        });
        true
    }

    #[cfg(test)]
    pub(crate) fn insert_batch(
        &mut self,
        channel_id: ChannelId,
        batch: Vec<Arc<Message>>,
        end: End,
        reached_end: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        let limit = self.limits.messages;
        if let Some(window) = self.windows.get_mut(&channel_id) {
            window.merge(channel_id, batch, end, reached_end, limit, events);
        }
    }

    // None if there's nothing to load: no window, one at the present for Newer, a fresh one
    // for Refresh.
    pub(crate) fn begin_load(
        &mut self,
        channel: ChannelId,
        kind: LoadKind,
        placing: u64,
        newest: Option<MessageId>,
        events: &mut Vec<StoreEvent>,
    ) -> Option<LoadTicket> {
        if matches!(kind, LoadKind::Latest | LoadKind::Around(_)) {
            self.view(channel, events);
        }
        let window = self.windows.get_mut(&channel)?;
        let first = window.messages.first().map(|message| message.id);
        let last = window.messages.last().map(|message| message.id);
        let (kind, cursor) = match (kind, first, last) {
            (LoadKind::Older, Some(first), _) => (kind, Cursor::Before(first)),
            (LoadKind::Newer, _, Some(last)) if !window.latest => (kind, Cursor::After(last)),
            (LoadKind::Newer, _, Some(_)) => return None,
            (LoadKind::Refresh, ..) if !window.stale => return None,
            (LoadKind::Refresh, ..) => (kind, Cursor::Latest),
            (LoadKind::Around(id), ..) => (kind, Cursor::Around(id)),
            (LoadKind::Latest | LoadKind::Older | LoadKind::Newer, ..) => {
                (LoadKind::Latest, Cursor::Latest)
            }
        };
        let holds = matches!(kind, LoadKind::Latest | LoadKind::Refresh | LoadKind::Newer)
            && !window.appends_live();
        if holds {
            window.holding += 1;
        }
        if matches!(kind, LoadKind::Latest | LoadKind::Around(_)) {
            window.placing = window.placing.max(placing);
        }
        if matches!(kind, LoadKind::Around(_)) {
            window.around_placing = window.around_placing.max(placing);
        }
        window.loading += 1;
        Some(LoadTicket {
            channel,
            kind,
            cursor,
            generation: window.generation,
            placing,
            holds,
            newest,
        })
    }

    pub(crate) fn finish_load(
        &mut self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        users: &HashMap<UserId, Arc<User>>,
        reached_end: bool,
        events: &mut Vec<StoreEvent>,
    ) -> PageOutcome {
        let window_limit = self.limits.messages;
        let generation = self.next_generation();
        let channel_id = ticket.channel;
        // Asked for after the load began: what it lacks up to the channel's newest was deleted.
        let through = page
            .iter()
            .map(|message| message.id)
            .max()
            .max(ticket.newest);
        let Some(window) = self
            .windows
            .get_mut(&channel_id)
            .filter(|window| window.generation == ticket.generation)
        else {
            return PageOutcome::Outdated;
        };
        let was_stale = window.stale;
        if ticket.holds {
            window.holding = window.holding.saturating_sub(1);
        }
        window.loading = window.loading.saturating_sub(1);
        // A trim or delete at the end the page continues from would leave a gap.
        let moved = match ticket.cursor {
            Cursor::Before(first) => {
                window.messages.first().map(|message| message.id) != Some(first)
            }
            Cursor::After(last) => window.messages.last().map(|message| message.id) != Some(last),
            Cursor::Latest | Cursor::Around(_) => false,
        };
        let overtaken =
            matches!(ticket.kind, LoadKind::Around(_)) && ticket.placing < window.placing;
        if moved || overtaken {
            window.forget_missed();
            window.release_or_drop_held(channel_id, window_limit, events);
            return if moved {
                PageOutcome::Outdated
            } else {
                PageOutcome::Overtaken
            };
        }
        let mut page: Vec<Arc<Message>> = page
            .into_iter()
            .map(|message| {
                Arc::new(Message::from_wire(message, &mut |user| {
                    intern(user, users, &window.messages)
                }))
            })
            .collect();
        page.sort_by_key(|message| message.id);
        for missed in &window.missed {
            match missed {
                Missed::Deleted(id) => page.retain(|message| message.id != *id),
                Missed::Updated(update) => {
                    let Ok(index) = Window::position(&page, update.id) else {
                        continue;
                    };
                    let page_is_newer =
                        match (update.edited_timestamp, page[index].edited_timestamp) {
                            (Some(Some(edit)), Some(shown)) => shown > edit,
                            _ => false,
                        };
                    if !page_is_newer {
                        page[index] =
                            Arc::new(page[index].patch((**update).clone(), &mut |user| {
                                intern(user, users, &window.messages)
                            }));
                    }
                }
            }
        }
        window.forget_missed();
        match ticket.kind {
            LoadKind::Older => {
                window.merge(
                    channel_id,
                    page,
                    End::Older,
                    reached_end,
                    window_limit,
                    events,
                );
            }
            LoadKind::Newer => {
                window.merge(
                    channel_id,
                    page,
                    End::Newer,
                    reached_end,
                    window_limit,
                    events,
                );
                // Older parts of a stale window aren't re-checked, like the official client.
                window.stale &= !reached_end;
                if reached_end {
                    window.present_through = window.present_through.max(through);
                }
            }
            LoadKind::Latest if window.appends_live() => {
                window.merge(
                    channel_id,
                    page,
                    End::Older,
                    reached_end,
                    window_limit,
                    events,
                );
                window.present_through = window.present_through.max(through);
            }
            LoadKind::Refresh if !window.stale => {}
            LoadKind::Latest | LoadKind::Refresh if window.stale && window.reaches(&page) => {
                let first = page.first().map(|message| message.id);
                window.reconcile(channel_id, page, reached_end, window_limit, events);
                window.present_through = window.present_through.max(through);
                if window.from_disk {
                    window.drop_before(first, channel_id, events);
                }
            }
            LoadKind::Refresh => {
                window.latest = false;
                events.push(StoreEvent::MessagesStale { channel_id });
            }
            // Replacing would change the generation and drop the newer Around page.
            LoadKind::Latest
                if window.around_placing > ticket.placing
                    && window.placing == window.around_placing =>
            {
                // As for an aborted load: should the Around load fail, nothing would refresh it.
                if window.holding == 0 && window.stale && window.latest {
                    window.latest = false;
                    events.push(StoreEvent::MessagesStale { channel_id });
                }
                window.release_or_drop_held(channel_id, window_limit, events);
                return PageOutcome::Overtaken;
            }
            LoadKind::Latest | LoadKind::Around(_) => {
                if !window.messages.is_empty() {
                    events.push(StoreEvent::MessagesCleared { channel_id });
                }
                let at_present = ticket.kind == LoadKind::Latest;
                let held = std::mem::take(&mut window.held);
                let held_deleted = window.held_deleted;
                // As for a release: the page can't join held messages past a dropped one.
                let past_dropped =
                    at_present && page.last().map(|message| message.id) < window.held_dropped;
                if past_dropped {
                    page.clear();
                }
                let outbox = std::mem::take(&mut window.outbox);
                *window = Window {
                    generation,
                    outbox,
                    placing: window.placing,
                    around_placing: window.around_placing,
                    ..Window::default()
                };
                let mut added = Added::default();
                for message in &page {
                    added.add(message.id);
                }
                window.messages = page;
                window.oldest = at_present && reached_end && !past_dropped;
                window.latest = at_present;
                window.present_through = through.max(held_deleted).filter(|_| at_present);
                if at_present {
                    let newest = window.messages.last().map(|message| message.id);
                    for message in held
                        .into_iter()
                        .filter(|message| newest.is_none_or(|newest| message.id > newest))
                    {
                        added.add(message.id);
                        window.messages.push(message);
                    }
                }
                added.push(channel_id, events);
                window.trim(channel_id, window_limit, End::Older, events);
            }
        }
        window.filled = true;
        window.from_disk = false;
        window.release_or_drop_held(channel_id, window_limit, events);
        // A refresh that found nothing new sends no other event.
        if was_stale && !window.stale {
            events.push(StoreEvent::MessagesRefreshed { channel_id });
        }
        PageOutcome::Applied
    }

    // Escape or a mark as read, where no Latest load runs to drop the page.
    pub(crate) fn supersede_around(&mut self, channel: ChannelId, placing: u64) {
        if let Some(window) = self.windows.get_mut(&channel) {
            window.placing = window.placing.max(placing);
        }
    }

    // Cached messages show while the window's first page loads; they may be outdated, so the
    // window is stale and the page reconciles or replaces them. Too late, they change nothing.
    pub(crate) fn fill_from_cache(
        &mut self,
        ticket: LoadTicket,
        messages: Vec<Arc<Message>>,
        events: &mut Vec<StoreEvent>,
    ) {
        let channel_id = ticket.channel;
        let Some(window) = self.windows.get_mut(&channel_id).filter(|window| {
            window.generation == ticket.generation
                && window.loading > 0
                && window.messages.is_empty()
                && window.latest
        }) else {
            return;
        };
        let mut added = Added::default();
        for message in &messages {
            added.add(message.id);
        }
        window.messages = messages;
        window.messages.sort_by_key(|message| message.id);
        window.stale = true;
        window.from_disk = true;
        added.push(channel_id, events);
        events.push(StoreEvent::MessagesStale { channel_id });
    }

    pub(crate) fn abort_load(&mut self, ticket: LoadTicket, events: &mut Vec<StoreEvent>) {
        let limit = self.limits.messages;
        let channel_id = ticket.channel;
        let Some(window) = self
            .windows
            .get_mut(&channel_id)
            .filter(|window| window.generation == ticket.generation)
        else {
            return;
        };
        window.loading = window.loading.saturating_sub(1);
        window.forget_missed();
        if !ticket.holds {
            return;
        }
        window.holding = window.holding.saturating_sub(1);
        // A stale window that can't be refreshed would hold live messages forever.
        if window.holding == 0 && window.stale && window.latest {
            window.latest = false;
            events.push(StoreEvent::MessagesStale { channel_id });
        }
        window.release_or_drop_held(channel_id, limit, events);
    }

    pub(crate) fn update(
        &mut self,
        update: MessageUpdate,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(window) = self.windows.get_mut(&update.channel_id) else {
            return;
        };
        if window.loading > 0 {
            window
                .missed
                .push(Missed::Updated(Box::new(update.clone())));
        }
        if let Ok(index) = Window::position(&window.messages, update.id) {
            let known = window.messages[index].clone();
            let next =
                Arc::new(known.patch(update, &mut |user| intern(user, users, &window.messages)));
            if next != known {
                window.messages[index] = next.clone();
                events.push(StoreEvent::MessageUpdated(next));
            }
        } else if let Ok(index) = Window::position(&window.held, update.id) {
            let next =
                window.held[index].patch(update, &mut |user| Arc::new(User::from_wire(user)));
            window.held[index] = Arc::new(next);
        }
    }

    pub(crate) fn delete(
        &mut self,
        channel_id: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        if window.loading > 0 {
            window.missed.push(Missed::Deleted(id));
        }
        if let Ok(index) = Window::position(&window.messages, id) {
            window.messages.remove(index);
            if index == window.messages.len() && window.latest && !window.stale {
                window.present_through = window.present_through.max(Some(id));
            }
            events.push(StoreEvent::MessageDeleted {
                channel_id,
                message_id: id,
            });
        } else if let Ok(index) = Window::position(&window.held, id) {
            window.held.remove(index);
            window.held_deleted = window.held_deleted.max(Some(id));
        }
    }

    pub(crate) fn drop_channel(&mut self, channel: ChannelId) {
        self.starters.remove(&channel);
        let generation = self.next_generation();
        let Some(window) = self.windows.get_mut(&channel) else {
            return;
        };
        // Our unsent messages stay readable and retryable, so a UI can offer their text again.
        if !window.outbox.is_empty() {
            let outbox = std::mem::take(&mut window.outbox);
            *window = Window {
                generation,
                outbox,
                ..Window::default()
            };
            return;
        }
        self.windows.remove(&channel);
        self.order.retain(|viewed| *viewed != channel);
    }

    pub(crate) fn channels(&self) -> impl Iterator<Item = ChannelId> + '_ {
        self.windows.keys().copied()
    }

    pub(crate) fn stale_channels(&self) -> Vec<ChannelId> {
        let mut channels: Vec<ChannelId> = self
            .windows
            .iter()
            .filter(|(_, window)| window.stale && window.latest)
            .map(|(channel, _)| *channel)
            .collect();
        channels.sort_unstable();
        channels
    }

    pub(crate) fn mark_stale(&mut self, events: &mut Vec<StoreEvent>) {
        let mut channels: Vec<ChannelId> = self.windows.keys().copied().collect();
        channels.sort_unstable();
        for channel_id in channels {
            if let Some(window) = self.windows.get_mut(&channel_id) {
                window.stale = true;
                events.push(StoreEvent::MessagesStale { channel_id });
            }
        }
    }
}

fn intern(
    user: model::User,
    directory: &HashMap<UserId, Arc<User>>,
    recent: &[Arc<Message>],
) -> Arc<User> {
    let user = User::from_wire(user);
    if let Some(known) = directory.get(&user.id)
        && **known == user
    {
        return known.clone();
    }
    recent
        .iter()
        .rev()
        .take(RECENT_AUTHORS)
        .flat_map(|message| std::iter::once(&message.author).chain(message.mentions.iter()))
        .find(|known| ***known == user)
        .cloned()
        .unwrap_or_else(|| Arc::new(user))
}

#[cfg(test)]
mod tests;
