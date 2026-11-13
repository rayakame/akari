use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use super::apply::wall_millis;
use super::events::StoreEvent;
use super::reactions::{ReactionChange, react};
use super::replies::{ReplyPreview, ReplyTarget, ReplyTargets};
use super::types::{Delivery, Message, StarterMessage, User};
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
    // The window's missed changes when the load began; its page has those.
    missed: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedWindow {
    // Milliseconds since the Unix epoch, so it orders against earlier sessions' windows.
    pub(crate) viewed_at: i64,
    pub(crate) messages: Vec<Arc<Message>>,
    // By the target's channel and ID; not loaded ones are left out.
    pub(crate) targets: CachedTargets,
}

pub(crate) type CachedTargets = HashMap<(ChannelId, MessageId), ReplyTarget>;

#[derive(Debug, Clone, Default)]
pub(crate) struct CachedMessages {
    pub(crate) messages: Vec<Arc<Message>>,
    pub(crate) targets: CachedTargets,
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
    // The outbox's replies that don't ping their target's author.
    quiet: Vec<MessageId>,
    loading: u32,
    // Edits, deletes and reactions while a load runs, each with its number; a page may predate
    // those that came after its load began.
    missed: Vec<(u64, Missed)>,
    misses: u64,
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
    Reacted(MessageId, ReactionChange),
}

pub(crate) struct Outgoing {
    pub(crate) message: Arc<Message>,
    pub(crate) mention: bool,
}

pub(crate) struct Answered {
    pub(crate) target: MessageId,
    pub(crate) preview: Arc<ReplyPreview>,
    // Kept beside the outbox for a retry.
    pub(crate) mention: bool,
}

impl Window {
    fn outgoing(&self, message: &Arc<Message>) -> Outgoing {
        Outgoing {
            message: message.clone(),
            mention: !self.quiet.contains(&message.id),
        }
    }

    fn appends_live(&self) -> bool {
        self.latest && !self.stale && self.holding == 0
    }

    fn holds_live(&self) -> bool {
        self.holding > 0 || (self.stale && self.latest)
    }

    fn miss(&mut self, missed: Missed) {
        if self.loading > 0 {
            self.misses += 1;
            self.missed.push((self.misses, missed));
        }
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

    // The page is the channel's newest; within its range it is the truth. Returns the rows it
    // lacks, which were deleted.
    fn reconcile(
        &mut self,
        channel_id: ChannelId,
        page: Vec<Arc<Message>>,
        reached_end: bool,
        limit: usize,
        events: &mut Vec<StoreEvent>,
    ) -> Vec<MessageId> {
        let mut deleted = Vec::new();
        if let (Some(first), Some(last)) = (page.first(), page.last()) {
            let (first, last) = (first.id, last.id);
            let ids: HashSet<MessageId> = page.iter().map(|message| message.id).collect();
            self.messages.retain(|message| {
                let gone = (first..=last).contains(&message.id) && !ids.contains(&message.id);
                if gone {
                    deleted.push(message.id);
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
        deleted
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
    // Sent along with the thread's first message, or what the fetch found.
    starters: HashMap<ChannelId, Starter>,
    targets: ReplyTargets,
    // The message each channel's pending reply answers: its target stays without the window.
    holds: HashMap<ChannelId, MessageId>,
}

// `Some(None)`: Discord sent it along as deleted.
type Referenced = Option<Option<Box<model::Message>>>;

#[derive(Debug, Clone, PartialEq)]
enum Starter {
    Fetching,
    Found(Arc<Message>),
    Gone,
    // A request that failed; the thread's next page asks again.
    Failed,
}

// The starter row's ID and what it showed.
type ShownStarter = (Option<MessageId>, Option<Starter>);

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
            targets: ReplyTargets::default(),
            holds: HashMap::new(),
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
        Some(match self.starters.get(&thread) {
            Some(Starter::Found(message)) => StarterMessage::Loaded(fresh(message)),
            Some(Starter::Gone | Starter::Failed) => StarterMessage::Unavailable,
            Some(Starter::Fetching) | None if origin.is_none() => StarterMessage::Unavailable,
            Some(Starter::Fetching) | None => StarterMessage::Loading,
        })
    }

    // The message `thread`'s starter row `id` shows in its own place, once it is loaded.
    pub(crate) fn shown_by_row(&self, thread: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        let row = self.message(thread, id)?;
        if row.kind != MessageType::ThreadStarterMessage {
            return None;
        }
        match self.starter_message(thread) {
            Some(StarterMessage::Loaded(starter)) => Some(starter),
            _ => None,
        }
    }

    // `id` of `channel` as its window or a viewed thread's starter shows it.
    pub(crate) fn shown(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.message(channel, id).or_else(|| {
            self.windows
                .keys()
                .find_map(|thread| match self.starter_message(*thread) {
                    Some(StarterMessage::Loaded(starter))
                        if starter.channel_id == channel && starter.id == id =>
                    {
                        Some(starter)
                    }
                    _ => None,
                })
        })
    }

    // The starter rows of viewed threads that show `id` of `channel`.
    pub(crate) fn starter_rows_showing(
        &self,
        channel: ChannelId,
        id: MessageId,
    ) -> Vec<Arc<Message>> {
        self.windows
            .keys()
            .filter(|thread| {
                matches!(
                    self.starter_message(**thread),
                    Some(StarterMessage::Loaded(starter))
                        if starter.channel_id == channel && starter.id == id
                )
            })
            .filter_map(|thread| self.starter_row(*thread).cloned())
            .collect()
    }

    pub(crate) fn wanted_starter(&self, thread: ChannelId) -> Option<(ChannelId, MessageId)> {
        let row = self.starter_row(thread)?;
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
            StarterFetch::Found(mut message) => {
                let referenced = message.referenced_message.take();
                let found = Arc::new(Message::from_wire(*message, &mut |user| {
                    intern(user, users, &[])
                }));
                self.keep_referenced(&found, referenced, users, events);
                Starter::Found(found)
            }
            StarterFetch::Gone => Starter::Gone,
            StarterFetch::Failed => Starter::Failed,
        };
        self.starters.insert(thread, settled);
        if let Some(row) = self.starter_row(thread) {
            events.push(StoreEvent::MessageUpdated(row.clone()));
        }
    }

    // After a failed page, whose fetch would fail too; the next page asks again.
    pub(crate) fn fail_starter(&mut self, thread: ChannelId, events: &mut Vec<StoreEvent>) {
        if self.starters.contains_key(&thread) || self.wanted_starter(thread).is_none() {
            return;
        }
        self.starters.insert(thread, Starter::Failed);
        if let Some(row) = self.starter_row(thread) {
            events.push(StoreEvent::MessageUpdated(row.clone()));
        }
    }

    fn starter_shown(&self, thread: ChannelId) -> ShownStarter {
        (
            self.starter_row(thread).map(|row| row.id),
            self.starters.get(&thread).cloned(),
        )
    }

    // A row that stays equal while its starter changes has no event of its own.
    fn announce_starter(
        &self,
        thread: ChannelId,
        before: &ShownStarter,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(row) = self.starter_row(thread) else {
            return;
        };
        if before.0 != Some(row.id) || self.starters.get(&thread) == before.1.as_ref() {
            return;
        }
        let announced = events.iter().any(|event| {
            matches!(event, StoreEvent::MessageUpdated(updated)
                if updated.id == row.id && updated.channel_id == thread)
        });
        if !announced {
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
                self.drop_starter(evicted);
                self.prune_targets(evicted);
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
                let messages = window.messages[skip..].to_vec();
                let targets = messages
                    .iter()
                    .filter_map(|message| message.replied_to())
                    .map(|(channel, id)| ((channel, id), self.targets.get(channel, id)))
                    .filter(|(_, target)| *target != ReplyTarget::NotLoaded)
                    .collect();
                Some((
                    *channel,
                    CachedWindow {
                        viewed_at: window.viewed_at,
                        messages,
                        targets,
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

    pub(crate) fn messages(&self, channel: ChannelId) -> &[Arc<Message>] {
        self.windows
            .get(&channel)
            .map_or(&[], |window| window.messages.as_slice())
    }

    pub(crate) fn message(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        let messages = &self.windows.get(&channel)?.messages;
        let index = Window::position(messages, id).ok()?;
        messages.get(index).cloned()
    }

    pub(crate) fn pending(&self, channel: ChannelId, id: MessageId) -> Option<Outgoing> {
        let window = self.windows.get(&channel)?;
        window
            .outbox
            .iter()
            .find(|message| message.id == id && message.delivery == Delivery::Pending)
            .map(|message| window.outgoing(message))
    }

    // The newest loaded message; pending and failed ones aren't in `messages`.
    pub(crate) fn newest_message(&self, channel: ChannelId) -> Option<Arc<Message>> {
        self.windows.get(&channel)?.messages.last().cloned()
    }

    // The loaded row of `parent` that shows `thread`'s box: the one with the thread's ID.
    pub(crate) fn box_row(&self, parent: ChannelId, thread: ChannelId) -> Option<Arc<Message>> {
        let messages = &self.windows.get(&parent)?.messages;
        let index = Window::position(messages, thread.cast()).ok()?;
        messages
            .get(index)
            .filter(|message| message.has_thread())
            .cloned()
    }

    pub(crate) fn has(&self, channel: ChannelId) -> bool {
        self.windows.contains_key(&channel)
    }

    // A loaded "started a thread" line of `parent` names `thread`.
    pub(crate) fn names_thread(&self, parent: ChannelId, thread: ChannelId) -> bool {
        self.windows.get(&parent).is_some_and(|window| {
            window.messages.iter().any(|message| {
                message.kind == MessageType::ThreadCreated
                    && message
                        .reference
                        .as_deref()
                        .is_some_and(|reference| reference.channel_id == Some(thread))
            })
        })
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
        mut message: model::Message,
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
        let referenced = message.referenced_message.take();
        let message = Arc::new(Message::from_wire(message, &mut |user| {
            intern(user, users, &window.messages)
        }));
        let starter = self.starter_shown(channel_id);
        self.keep_referenced(&message, referenced, users, events);
        self.targets.arrived(&message, events);
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        match pending {
            Some((pending_id, index)) => {
                window.outbox.remove(index);
                window.quiet.retain(|quiet| *quiet != pending_id);
                window.place(channel_id, message.clone(), limit, false, events);
                events.push(StoreEvent::MessageReplaced {
                    channel_id,
                    pending_id,
                    message,
                });
            }
            None => window.place(channel_id, message, limit, true, events),
        }
        self.announce_starter(channel_id, &starter, events);
        self.trim_targets(channel_id);
    }

    fn keep_referenced(
        &mut self,
        message: &Message,
        referenced: Referenced,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        if message.kind == MessageType::ThreadStarterMessage {
            match referenced {
                Some(Some(mut original)) => {
                    let answered = original.referenced_message.take();
                    let original = Arc::new(Message::from_wire(*original, &mut |user| {
                        intern(user, users, &[])
                    }));
                    self.keep_referenced(&original, answered, users, events);
                    self.starters
                        .insert(message.channel_id, Starter::Found(original));
                }
                Some(None) => {
                    self.starters.insert(message.channel_id, Starter::Gone);
                }
                None => {}
            }
            return;
        }
        let Some((channel, id)) = message.replied_to() else {
            return;
        };
        let target = match referenced {
            Some(Some(original)) => {
                let preview = ReplyPreview::from_wire(*original, &mut |user| {
                    intern(user, users, self.messages(channel))
                });
                ReplyTarget::Loaded(Arc::new(preview))
            }
            Some(None) => ReplyTarget::Deleted,
            None => match self.message(channel, id) {
                Some(loaded) => ReplyTarget::Loaded(Arc::new(ReplyPreview::of(&loaded))),
                None => {
                    self.targets.name(channel, id);
                    return;
                }
            },
        };
        self.targets.set(channel, id, target, events);
    }

    pub(crate) fn target_gone(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        self.targets.delete(channel, id, events);
    }

    pub(crate) fn reply_target(&self, channel: ChannelId, id: MessageId) -> ReplyTarget {
        self.targets.get(channel, id)
    }

    #[cfg(test)]
    pub(crate) fn targets_len(&self, channel: ChannelId) -> usize {
        self.targets.len(channel)
    }

    fn named_targets(&self, channel: ChannelId) -> HashSet<MessageId> {
        let rows = self.windows.values().flat_map(|window| {
            window
                .messages
                .iter()
                .chain(&window.held)
                .chain(&window.outbox)
        });
        let starters = self.starters.values().filter_map(|starter| match starter {
            Starter::Found(message) => Some(message),
            _ => None,
        });
        rows.chain(starters)
            .filter_map(|message| message.replied_to())
            .filter(|(named, _)| *named == channel)
            .map(|(_, id)| id)
            .chain(self.holds.get(&channel).copied())
            .collect()
    }

    pub(crate) fn hold_reply(&mut self, target: &Message, events: &mut Vec<StoreEvent>) {
        let channel = target.channel_id;
        let preview = Arc::new(ReplyPreview::of(target));
        self.targets
            .set(channel, target.id, ReplyTarget::Loaded(preview), events);
        self.holds.insert(channel, target.id);
        self.loosen_targets(channel);
    }

    pub(crate) fn release_reply(&mut self, channel: ChannelId) {
        if self.holds.remove(&channel).is_some() {
            self.loosen_targets(channel);
        }
    }

    pub(crate) fn release_replies(&mut self) {
        let channels: Vec<ChannelId> = self.holds.drain().map(|(channel, _)| channel).collect();
        for channel in channels {
            self.loosen_targets(channel);
        }
    }

    pub(crate) fn held_reply(&self, channel: ChannelId) -> Option<MessageId> {
        self.holds.get(&channel).copied()
    }

    pub(crate) fn held_preview(
        &self,
        channel: ChannelId,
        target: MessageId,
    ) -> Option<Arc<ReplyPreview>> {
        if self.held_reply(channel) != Some(target) {
            return None;
        }
        match self.targets.get(channel, target) {
            ReplyTarget::Loaded(preview) => Some(preview),
            ReplyTarget::Deleted | ReplyTarget::NotLoaded => None,
        }
    }

    fn loosen_targets(&mut self, channel: ChannelId) {
        if self.windows.contains_key(&channel) {
            self.trim_targets(channel);
        } else {
            self.prune_targets(channel);
        }
    }

    fn trim_targets(&mut self, channel: ChannelId) {
        if self.targets.over_limit(channel) {
            let named = self.named_targets(channel);
            self.targets.trim(channel, |id| named.contains(&id));
        }
    }

    fn drop_starter(&mut self, thread: ChannelId) {
        let answered = match self.starters.remove(&thread) {
            Some(Starter::Found(starter)) => starter.replied_to(),
            _ => None,
        };
        if let Some((channel, _)) = answered {
            self.prune_targets(channel);
        }
    }

    fn prune_targets(&mut self, channel: ChannelId) {
        let named = self.named_targets(channel);
        self.targets.prune(channel, |id| named.contains(&id));
    }

    pub(crate) fn queue(
        &mut self,
        channel: ChannelId,
        message: Arc<Message>,
        answers: Option<Answered>,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(window) = self.windows.get_mut(&channel) else {
            return;
        };
        let Some(answered) = answers else {
            window.outbox.push(message.clone());
            events.push(StoreEvent::MessageInserted(message));
            return;
        };
        if !answered.mention {
            window.quiet.push(message.id);
        }
        window.outbox.push(message.clone());
        let target = ReplyTarget::Loaded(answered.preview);
        self.targets.set(channel, answered.target, target, events);
        events.push(StoreEvent::MessageInserted(message));
        // The row names the target from here on.
        self.holds.remove(&channel);
        self.trim_targets(channel);
    }

    fn set_delivery(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        from: Delivery,
        to: Delivery,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Outgoing> {
        let window = self.windows.get_mut(&channel)?;
        let queued = window
            .outbox
            .iter_mut()
            .find(|queued| queued.id == id && queued.delivery == from)?;
        let mut next = (**queued).clone();
        next.delivery = to;
        *queued = Arc::new(next);
        events.push(StoreEvent::MessageUpdated(queued.clone()));
        let queued = queued.clone();
        Some(window.outgoing(&queued))
    }

    pub(crate) fn fail(&mut self, channel: ChannelId, id: MessageId, events: &mut Vec<StoreEvent>) {
        self.set_delivery(channel, id, Delivery::Pending, Delivery::Failed, events);
    }

    pub(crate) fn retry(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Outgoing> {
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
        window.quiet.retain(|quiet| *quiet != id);
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
            missed: window.misses,
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
        let (mut page, referenced): (Vec<Arc<Message>>, Vec<Referenced>) = page
            .into_iter()
            .map(|mut message| {
                let referenced = message.referenced_message.take();
                let message =
                    Message::from_wire(message, &mut |user| intern(user, users, &window.messages));
                (Arc::new(message), referenced)
            })
            .unzip();
        let starter = self.starter_shown(channel_id);
        for (message, referenced) in page.iter().zip(referenced) {
            self.keep_referenced(message, referenced, users, events);
        }
        for message in &page {
            self.targets.arrived(message, events);
        }
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return PageOutcome::Outdated;
        };
        page.sort_by_key(|message| message.id);
        let since_the_load = window
            .missed
            .iter()
            .filter(|(number, _)| *number > ticket.missed);
        for (_, missed) in since_the_load {
            match missed {
                Missed::Deleted(id) => {
                    page.retain(|message| message.id != *id);
                    self.targets.delete(channel_id, *id, events);
                }
                Missed::Reacted(id, change) => {
                    let Ok(index) = Window::position(&page, *id) else {
                        continue;
                    };
                    // A copy that was current counted every reaction since; the page may or may
                    // not have this one.
                    let live = [&window.messages, &window.held]
                        .into_iter()
                        .find_map(|copies| {
                            let index = Window::position(copies, *id).ok()?;
                            copies.get(index)
                        })
                        .filter(|_| !was_stale);
                    match live {
                        Some(live) if live.reactions != page[index].reactions => {
                            page[index] = Arc::new(Message {
                                reactions: live.reactions.clone(),
                                ..(*page[index]).clone()
                            });
                        }
                        Some(_) => {}
                        None => {
                            react(&mut page[index], std::slice::from_ref(change));
                        }
                    }
                }
                Missed::Updated(update) => {
                    self.targets.update(
                        update,
                        &mut |user| intern(user, users, &window.messages),
                        events,
                    );
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
                let deleted = window.reconcile(channel_id, page, reached_end, window_limit, events);
                for id in deleted {
                    self.targets.delete(channel_id, id, events);
                }
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
                self.announce_starter(channel_id, &starter, events);
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
                let quiet = std::mem::take(&mut window.quiet);
                *window = Window {
                    generation,
                    outbox,
                    quiet,
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
        self.announce_starter(channel_id, &starter, events);
        self.trim_targets(channel_id);
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
        cached: CachedMessages,
        events: &mut Vec<StoreEvent>,
    ) {
        let CachedMessages { messages, targets } = cached;
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
        // What this session learned since is newer than the file.
        for ((channel, id), target) in targets {
            if self.targets.get(channel, id) == ReplyTarget::NotLoaded {
                self.targets.set(channel, id, target, events);
            }
        }
        added.push(channel_id, events);
        events.push(StoreEvent::MessagesStale { channel_id });
        self.trim_targets(channel_id);
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
        let recent = self
            .windows
            .get(&update.channel_id)
            .map_or(&[][..], |window| window.messages.as_slice());
        self.targets
            .update(&update, &mut |user| intern(user, users, recent), events);
        let Some(window) = self.windows.get_mut(&update.channel_id) else {
            return;
        };
        window.miss(Missed::Updated(Box::new(update.clone())));
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

    pub(crate) fn react(
        &mut self,
        channel_id: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        let mut row = false;
        if let Some(window) = self.windows.get_mut(&channel_id) {
            for change in changes {
                window.miss(Missed::Reacted(id, change.clone()));
            }
            if let Ok(index) = Window::position(&window.messages, id) {
                row = react(&mut window.messages[index], changes);
            } else if let Ok(index) = Window::position(&window.held, id) {
                react(&mut window.held[index], changes);
            }
        }
        if row {
            events.push(StoreEvent::MessageReactionsChanged {
                channel_id,
                message_id: id,
            });
        }
        if self.react_to_starters(channel_id, id, changes) | row {
            events.extend(self.starter_rows_showing(channel_id, id).iter().map(|row| {
                StoreEvent::MessageReactionsChanged {
                    channel_id: row.channel_id,
                    message_id: row.id,
                }
            }));
        }
        row
    }

    fn react_to_starters(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
    ) -> bool {
        let mut changed = false;
        for starter in self.starters.values_mut() {
            if let Starter::Found(message) = starter
                && message.channel_id == channel
                && message.id == id
            {
                changed |= react(message, changes);
            }
        }
        changed
    }

    pub(crate) fn delete(
        &mut self,
        channel_id: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        self.targets.delete(channel_id, id, events);
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        window.miss(Missed::Deleted(id));
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
        self.drop_starter(channel);
        self.holds.remove(&channel);
        self.targets.drop_channel(channel);
        let generation = self.next_generation();
        let Some(window) = self.windows.get_mut(&channel) else {
            return;
        };
        // Our unsent messages stay readable and retryable, so a UI can offer their text again.
        if !window.outbox.is_empty() {
            let outbox = std::mem::take(&mut window.outbox);
            let quiet = std::mem::take(&mut window.quiet);
            *window = Window {
                generation,
                outbox,
                quiet,
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
