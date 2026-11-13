use std::collections::HashMap;
use std::sync::Arc;

use super::convert::Intern;
use super::events::StoreEvent;
use super::types::{Message, User};
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, MessageReferenceType, MessageType, UserId};

// The official client's ReferencedMessageStore keeps this many per channel.
const LIMIT: usize = 100;

/// What a message to send answers: see [`Account::send_message`](crate::Account::send_message).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyTo {
    /// A message of the channel the reply goes to.
    pub message: MessageId,
    /// Whether the reply pings the message's author; a reply to the user's own message never
    /// does.
    pub mention: bool,
}

/// The message a reply answers, as its preview line needs it: see
/// [`Store::reply_target`](super::Store::reply_target).
// Exhaustive, so a new case can't reach the apps as one of these unnoticed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyTarget {
    Loaded(Arc<ReplyPreview>),
    /// Discord says it is gone, or it was deleted since.
    Deleted,
    /// Discord didn't send it along and no load has brought it.
    NotLoaded,
}

/// What a reply's one-line preview shows of the message it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReplyPreview {
    /// System kinds have their own wording; `ChatInputCommand` and `ContextMenuCommand` are
    /// a command's result.
    pub kind: MessageType,
    /// As sent with the message; the member's name and color are looked up by its ID in the
    /// target's guild, unless `webhook`.
    pub author: Arc<User>,
    /// The author is a webhook's name and avatar, not a user.
    pub webhook: bool,
    pub content: Box<str>,
    pub edited: bool,
    /// The message's flags, such as [`Message::IS_VOICE_MESSAGE`].
    pub flags: u64,
    pub has_attachments: bool,
    pub has_embeds: bool,
    pub has_sticker: bool,
    /// A forward: its content is in a snapshot Akari doesn't read.
    pub forwarded: bool,
}

impl ReplyPreview {
    pub(crate) fn of(message: &Message) -> Self {
        Self {
            kind: message.kind,
            author: message.author.clone(),
            webhook: message.webhook_id.is_some(),
            content: message.content.clone(),
            edited: message.edited_timestamp.is_some(),
            flags: message.flags,
            has_attachments: !message.attachments.is_empty(),
            has_embeds: !message.embeds.is_empty(),
            has_sticker: !message.stickers.is_empty(),
            forwarded: message
                .reference
                .as_deref()
                .is_some_and(|reference| reference.kind == MessageReferenceType::Forward),
        }
    }

    pub(crate) fn from_wire(message: model::Message, intern: Intern<'_>) -> Self {
        Self {
            kind: message.kind,
            author: intern(message.author),
            webhook: message.webhook_id.is_some(),
            content: message.content.into_boxed_str(),
            edited: message.edited_timestamp.is_some(),
            flags: message.flags,
            has_attachments: !message.attachments.is_empty(),
            has_embeds: !message.embeds.is_empty(),
            has_sticker: !message.sticker_items.is_empty(),
            forwarded: message
                .message_reference
                .is_some_and(|reference| reference.kind == MessageReferenceType::Forward),
        }
    }

    fn patch(&self, update: &MessageUpdate, intern: Intern<'_>) -> Self {
        let mut next = self.clone();
        next.kind = update.kind.unwrap_or(next.kind);
        if let Some(author) = &update.author {
            next.author = intern(author.clone());
        }
        next.webhook |= update.webhook_id.is_some();
        if let Some(content) = &update.content {
            next.content = content.as_str().into();
        }
        if let Some(edited) = update.edited_timestamp {
            next.edited = edited.is_some();
        }
        next.flags = update.flags.unwrap_or(next.flags);
        if let Some(attachments) = &update.attachments {
            next.has_attachments = !attachments.is_empty();
        }
        if let Some(embeds) = &update.embeds {
            next.has_embeds = !embeds.is_empty();
        }
        if let Some(stickers) = &update.sticker_items {
            next.has_sticker = !stickers.is_empty();
        }
        next
    }
}

impl Message {
    /// Whether the message's mentions name `user`. For a reply and its target's author: the
    /// reply pinged, or mentions them in its text, and the preview's name gets its "@".
    pub fn mentions_user(&self, user: UserId) -> bool {
        self.mentions.iter().any(|mentioned| mentioned.id == user)
    }

    /// The message this one answers, as [`Store::reply_target`](super::Store::reply_target)
    /// takes it: for a reply and a context-menu command's result, `None` for every other kind,
    /// whatever its reference.
    pub fn replied_to(&self) -> Option<(ChannelId, MessageId)> {
        if !matches!(
            self.kind,
            MessageType::Reply | MessageType::ContextMenuCommand
        ) {
            return None;
        }
        let reference = self.reference.as_deref()?;
        Some((
            reference.channel_id.unwrap_or(self.channel_id),
            reference.message_id?,
        ))
    }
}

#[cfg_attr(test, derive(PartialEq))]
struct Slot {
    target: ReplyTarget,
    // When a reply or the message itself last fed it; the oldest go first.
    fed: u64,
}

/// The targets of the replies the windows hold, per the target's channel.
#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ReplyTargets {
    channels: HashMap<ChannelId, HashMap<MessageId, Slot>>,
    feeds: u64,
}

impl ReplyTargets {
    pub(crate) fn get(&self, channel: ChannelId, id: MessageId) -> ReplyTarget {
        self.channels
            .get(&channel)
            .and_then(|targets| targets.get(&id))
            .map_or(ReplyTarget::NotLoaded, |slot| slot.target.clone())
    }

    fn has(&self, channel: ChannelId, id: MessageId) -> bool {
        self.channels
            .get(&channel)
            .is_some_and(|targets| targets.contains_key(&id))
    }

    #[cfg(test)]
    pub(crate) fn len(&self, channel: ChannelId) -> usize {
        self.channels.get(&channel).map_or(0, HashMap::len)
    }

    pub(crate) fn name(&mut self, channel: ChannelId, id: MessageId) {
        self.feeds += 1;
        self.channels
            .entry(channel)
            .or_default()
            .entry(id)
            .or_insert(Slot {
                target: ReplyTarget::NotLoaded,
                fed: 0,
            })
            .fed = self.feeds;
    }

    pub(crate) fn set(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        target: ReplyTarget,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.get(channel, id) != target {
            events.push(StoreEvent::ReplyTargetChanged {
                channel_id: channel,
                target_id: id,
            });
        }
        self.feeds += 1;
        self.channels.entry(channel).or_default().insert(
            id,
            Slot {
                target,
                fed: self.feeds,
            },
        );
    }

    pub(crate) fn arrived(&mut self, message: &Message, events: &mut Vec<StoreEvent>) {
        if self.has(message.channel_id, message.id) {
            let preview = Arc::new(ReplyPreview::of(message));
            self.set(
                message.channel_id,
                message.id,
                ReplyTarget::Loaded(preview),
                events,
            );
        }
    }

    pub(crate) fn update(
        &mut self,
        update: &MessageUpdate,
        intern: Intern<'_>,
        events: &mut Vec<StoreEvent>,
    ) {
        let ReplyTarget::Loaded(known) = self.get(update.channel_id, update.id) else {
            return;
        };
        let next = known.patch(update, intern);
        if next == *known {
            return;
        }
        let slot = self
            .channels
            .get_mut(&update.channel_id)
            .and_then(|targets| targets.get_mut(&update.id));
        if let Some(slot) = slot {
            slot.target = ReplyTarget::Loaded(Arc::new(next));
            events.push(StoreEvent::ReplyTargetChanged {
                channel_id: update.channel_id,
                target_id: update.id,
            });
        }
    }

    pub(crate) fn delete(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        let slot = self
            .channels
            .get_mut(&channel)
            .and_then(|targets| targets.get_mut(&id));
        if let Some(slot) = slot
            && slot.target != ReplyTarget::Deleted
        {
            slot.target = ReplyTarget::Deleted;
            events.push(StoreEvent::ReplyTargetChanged {
                channel_id: channel,
                target_id: id,
            });
        }
    }

    pub(crate) fn over_limit(&self, channel: ChannelId) -> bool {
        self.channels
            .get(&channel)
            .is_some_and(|targets| targets.len() > LIMIT)
    }

    // A target a held reply names always stays.
    pub(crate) fn trim(&mut self, channel: ChannelId, named: impl Fn(MessageId) -> bool) {
        self.keep(channel, LIMIT, named);
    }

    // The channel's window is gone: only what other windows' replies name stays.
    pub(crate) fn prune(&mut self, channel: ChannelId, named: impl Fn(MessageId) -> bool) {
        self.keep(channel, 0, named);
    }

    fn keep(&mut self, channel: ChannelId, limit: usize, named: impl Fn(MessageId) -> bool) {
        let Some(targets) = self.channels.get_mut(&channel) else {
            return;
        };
        let excess = targets.len().saturating_sub(limit);
        if excess > 0 {
            let mut loose: Vec<(u64, MessageId)> = targets
                .iter()
                .filter(|(id, _)| !named(**id))
                .map(|(id, slot)| (slot.fed, *id))
                .collect();
            loose.sort_unstable();
            for (_, id) in loose.into_iter().take(excess) {
                targets.remove(&id);
            }
        }
        if targets.is_empty() {
            self.channels.remove(&channel);
        }
    }

    pub(crate) fn drop_channel(&mut self, channel: ChannelId) {
        self.channels.remove(&channel);
    }
}
