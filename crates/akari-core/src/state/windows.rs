use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::events::StoreEvent;
use super::types::{Message, User};
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowLimits {
    pub(crate) channels: usize,
    pub(crate) messages: usize,
}

pub(crate) const DEFAULT_LIMITS: WindowLimits = WindowLimits {
    channels: 10,
    messages: 200,
};

// How far back a new message looks for an equal author to share.
const RECENT_AUTHORS: usize = 50;

// History loads will be the first caller outside tests.
#[cfg_attr(not(test), expect(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Older,
    Newer,
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
}

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
struct Window {
    messages: Vec<Arc<Message>>,
    latest: bool,
    oldest: bool,
    stale: bool,
    // Live messages that arrived while stale; appending them would leave a gap.
    held: Vec<Arc<Message>>,
}

impl Window {
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
}

impl Windows {
    pub(crate) fn new(limits: WindowLimits) -> Self {
        Self {
            limits,
            order: VecDeque::new(),
            windows: HashMap::new(),
        }
    }

    pub(crate) fn view(&mut self, channel: ChannelId, events: &mut Vec<StoreEvent>) {
        if let Some(index) = self.order.iter().position(|viewed| *viewed == channel) {
            self.order.remove(index);
        } else {
            self.windows.insert(
                channel,
                Window {
                    latest: true,
                    ..Window::default()
                },
            );
        }
        self.order.push_front(channel);
        while self.order.len() > self.limits.channels {
            if let Some(evicted) = self.order.pop_back() {
                self.windows.remove(&evicted);
                events.push(StoreEvent::MessagesCleared {
                    channel_id: evicted,
                });
            }
        }
    }

    pub(crate) fn snapshot(&self, channel: ChannelId) -> Option<MessageWindow> {
        let window = self.windows.get(&channel)?;
        Some(MessageWindow {
            messages: window.messages.clone(),
            latest: window.latest,
            oldest: window.oldest,
            stale: window.stale,
        })
    }

    pub(crate) fn message(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        let messages = &self.windows.get(&channel)?.messages;
        let index = Window::position(messages, id).ok()?;
        messages.get(index).cloned()
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
        let channel_id = message.channel_id;
        let Some(window) = self.windows.get(&channel_id) else {
            return;
        };
        if !window.latest {
            return;
        }
        let message = Arc::new(Message::from_wire(message, &mut |user| {
            intern(user, users, &window.messages)
        }));
        let limit = self.limits.messages;
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        match Window::position(&window.messages, message.id) {
            Ok(index) => {
                if window.messages[index] != message {
                    window.messages[index] = message.clone();
                    events.push(StoreEvent::MessageUpdated(message));
                }
            }
            Err(_) if window.stale => {
                upsert(&mut window.held, message);
                let excess = window.held.len().saturating_sub(limit);
                window.held.drain(..excess);
            }
            Err(index) => {
                window.messages.insert(index, message.clone());
                events.push(StoreEvent::MessageInserted(message));
                window.trim(channel_id, limit, End::Older, events);
            }
        }
    }

    // Callers pass the batch sorted by ID. History loads will be the first caller outside
    // tests.
    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn insert_batch(
        &mut self,
        channel_id: ChannelId,
        batch: Vec<Arc<Message>>,
        end: End,
        reached_end: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        let limit = self.limits.messages;
        let Some(window) = self.windows.get_mut(&channel_id) else {
            return;
        };
        let mut added: Option<(MessageId, MessageId)> = None;
        for message in batch {
            match Window::position(&window.messages, message.id) {
                Ok(index) => {
                    if window.messages[index] != message {
                        window.messages[index] = message.clone();
                        events.push(StoreEvent::MessageUpdated(message));
                    }
                }
                Err(index) => {
                    let id = message.id;
                    window.messages.insert(index, message);
                    added =
                        Some(added.map_or((id, id), |(first, last)| (first.min(id), last.max(id))));
                }
            }
        }
        if let Some((first, last)) = added {
            events.push(StoreEvent::MessagesLoaded {
                channel_id,
                first,
                last,
            });
        }
        match end {
            End::Older => window.oldest |= reached_end,
            End::Newer => window.latest |= reached_end,
        }
        let away_from_the_load = match end {
            End::Older => End::Newer,
            End::Newer => End::Older,
        };
        window.trim(channel_id, limit, away_from_the_load, events);
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
        if let Ok(index) = Window::position(&window.messages, id) {
            window.messages.remove(index);
            events.push(StoreEvent::MessageDeleted {
                channel_id,
                message_id: id,
            });
        } else if let Ok(index) = Window::position(&window.held, id) {
            window.held.remove(index);
        }
    }

    pub(crate) fn drop_channel(&mut self, channel: ChannelId) {
        if self.windows.remove(&channel).is_some() {
            self.order.retain(|viewed| *viewed != channel);
        }
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

// Shares one allocation between equal copies of a user: a DM recipient's directory entry,
// or the author or a mention of a recent message.
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
