use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use super::{Derived, Entities, Journal, ReadyDiff, State};
use crate::gateway::{ChannelUnread, MessageAck};
use crate::model::{
    self, ChannelId, ChannelType, GenericMarker, GuildId, MessageId, MessageType, Permissions,
    Snowflake, Timestamp, UserId,
};
use crate::state::badges::Dirty;
use crate::state::events::StoreEvent;
use crate::state::read_states::{Ack, AckBody, ReadState};
use crate::state::settings::NotificationLevel;
use crate::state::types::{Channel, Message, ThreadInfo};

const MENTION_ON_ALL_MESSAGES: u64 = 1 << 5;
// A channel and the thread beside it in the split view.
const READING_LIMIT: usize = 2;
const THREAD_JOIN_GRACE: i64 = 5_000;

// A channel the user is reading, with the newest row its list showed after the last write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Reading {
    channel: ChannelId,
    seen: Option<MessageId>,
    // Read since the last READY: setting it again reads nothing.
    read: bool,
}

// A channel marked unread locally: where its position went, and the count sent with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unread {
    pub(crate) position: MessageId,
    pub(crate) mentions: u32,
    pub(crate) before: Option<ReadState>,
    pub(crate) after: Option<ReadState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnreadNeeds {
    pub(crate) unarchive: bool,
    pub(crate) join: bool,
}

impl Entities {
    /// The channel's newest message is past where the user stopped reading.
    pub(crate) fn unread(&self, channel: &Channel) -> bool {
        channel
            .last_message_id
            .is_some_and(|last| last.get() > self.read_position(channel))
    }

    fn read_position(&self, channel: &Channel) -> u64 {
        match self.read_states.get(channel.id) {
            Some(state) => state.acked.map_or(0, Snowflake::get),
            None => self
                .start_position(channel)
                .unwrap_or_else(|| channel.last_message_id.map_or(0, Snowflake::get)),
        }
    }

    // Without a read state a guild channel is read up to the join, a DM until a message, and a
    // thread to the official client's start for it (`getAckTimestamp`, read-states.md).
    fn start_position(&self, channel: &Channel) -> Option<u64> {
        let guild_join = channel
            .guild_id
            .and_then(|guild| self.members.get(&guild)?.joined_at)
            .map(Timestamp::unix_millis);
        if !channel.is_thread() {
            let joined = guild_join?;
            return Some(Snowflake::<GenericMarker>::from_unix_millis(joined, 0).get());
        }
        let info = channel.thread.as_deref();
        let joined = info
            .and_then(|info| info.member?.joined_at)
            .map_or(-THREAD_JOIN_GRACE, |at| {
                at.unix_millis() - THREAD_JOIN_GRACE
            });
        let archived = info.map_or(0, |info| info.archive_timestamp.unix_millis() - 1);
        let mut start = joined.max(archived);
        if start <= 0 {
            start = channel.id.unix_millis() - 1;
        }
        let in_forum = channel
            .parent_id
            .and_then(|parent| self.channels.get(&parent))
            .is_some_and(|parent| parent.is_forum());
        if !in_forum && let Some(guild_join) = guild_join {
            start = start.max(guild_join);
        }
        // The last ID of that millisecond: a message within it is read, as by timestamps.
        Some(
            Snowflake::<GenericMarker>::from_unix_millis(start + 1, 0)
                .get()
                .saturating_sub(1),
        )
    }

    fn needs_ack(&self, channel: &Channel) -> bool {
        self.unread(channel)
            || self
                .read_states
                .get(channel.id)
                .is_some_and(|state| state.mentions > 0)
    }
}

impl State {
    #[cfg(test)]
    pub(crate) fn unread(&self, channel: ChannelId) -> bool {
        self.entities
            .channels
            .get(&channel)
            .is_some_and(|stored| self.entities.unread(stored))
    }

    pub(crate) fn read_state(&self, channel: ChannelId) -> Option<ReadState> {
        self.entities.read_states.get(channel)
    }

    // Runs before the message raises the channel's newest message, which a DM's start needs.
    pub(super) fn read_message(&mut self, message: &model::Message) {
        let Some(channel) = self.entities.channels.get(&message.channel_id).cloned() else {
            self.note_unknown_mention(message);
            return;
        };
        let joined = self.entities.start_position(&channel);
        let mut position = self.entities.read_position(&channel);
        let unstarted = joined.is_none() && self.entities.read_states.get(channel.id).is_none();
        // The CHANNEL_CREATE of a DM opened by this message can already name it as the newest.
        if unstarted && channel.last_message_id == Some(message.id.cast()) {
            position = message.id.get() - 1;
        }
        if message.id.get() <= position
            || !self
                .entities
                .read_states
                .first_sight(channel.id, message.id)
        {
            return;
        }
        if Some(message.author.id) == self.entities.me() {
            if message.kind != MessageType::PollResult
                && self.entities.read_states.read_up_to(channel.id, message.id)
            {
                self.post_ack_moved(channel.id);
                self.release(channel.id);
            }
            return;
        }
        let start = match joined {
            Some(joined) => Some(Snowflake::new(joined)),
            None if position == 0 => None,
            None => Some(Snowflake::new(position)),
        };
        if unstarted {
            self.entities.read_states.start(channel.id, start);
        }
        if self.counts_as_mention(&channel, message, self.now_millis()) {
            self.entities.read_states.mention(channel.id, start);
        }
    }

    // A mention that adds the user to a thread the store doesn't have yet: its THREAD_CREATE
    // can come later, and the count must be there when it does.
    fn note_unknown_mention(&mut self, message: &model::Message) {
        let (Some(me), Some(guild)) = (self.entities.me(), message.guild_id) else {
            return;
        };
        if !self.entities.guilds.contains_key(&guild)
            || message.author.id == me
            || !message.mentions.iter().any(|user| user.id == me)
            || self.entities.read_states.ignores(message.author.id)
            || !self
                .entities
                .read_states
                .first_sight(message.channel_id, message.id)
        {
            return;
        }
        let start = Snowflake::new(message.channel_id.get().saturating_sub(1));
        self.entities
            .read_states
            .mention(message.channel_id, Some(start));
    }

    fn counts_as_mention(
        &self,
        channel: &Channel,
        message: &model::Message,
        now_millis: i64,
    ) -> bool {
        let Some(me) = self.entities.me() else {
            return false;
        };
        if self.entities.read_states.ignores(message.author.id) {
            return false;
        }
        let now = Timestamp::from_unix_millis(now_millis);
        let mentions_me = message.mentions.iter().any(|user| user.id == me);
        let Some(guild) = channel.guild_id else {
            return message.kind != MessageType::RecipientRemove
                && (mentions_me || !self.channel_muted_at(channel.id, now));
        };
        // Only a direct mention adds the user to a thread; nothing reaches one they haven't joined.
        let member = channel.thread.as_deref().is_none_or(ThreadInfo::joined);
        let counts = mentions_me
            || (member
                && (self.mentions_through_everyone_or_roles(guild, message)
                    || self.counts_every_message(channel, guild, now)));
        counts
            && self
                .permissions(channel.id, now_millis)
                .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
    }

    fn mentions_through_everyone_or_roles(&self, guild: GuildId, message: &model::Message) -> bool {
        let (suppress_everyone, suppress_roles) = self
            .entities
            .settings
            .guild(Some(guild))
            .map_or((false, false), |settings| {
                (settings.suppress_everyone, settings.suppress_roles)
            });
        if message.mention_everyone && !suppress_everyone {
            return true;
        }
        !message.mention_roles.is_empty()
            && !suppress_roles
            && self.entities.members.get(&guild).is_some_and(|member| {
                member
                    .roles
                    .iter()
                    .any(|role| message.mention_roles.contains(role))
            })
    }

    fn counts_every_message(&self, channel: &Channel, guild: GuildId, now: Timestamp) -> bool {
        self.entities.settings.notification_flags & MENTION_ON_ALL_MESSAGES != 0
            && !matches!(
                channel.kind,
                ChannelType::GuildVoice | ChannelType::GuildStageVoice
            )
            && !self.channel_muted_at(channel.id, now)
            && !self.guild_settings(Some(guild)).muted_now(now)
            && self.notification_level(channel.id) == NotificationLevel::AllMessages
    }

    pub(super) fn apply_ack(&mut self, ack: &MessageAck) {
        if ack.ack_type != 0 {
            return;
        }
        let newest = self
            .entities
            .channels
            .get(&ack.channel_id)
            .and_then(|channel| channel.last_message_id)
            .map(Snowflake::get);
        let recount = self
            .recounts
            .remove(&ack.channel_id)
            .is_some_and(|at| at.get() == ack.message_id.get());
        if self.entities.read_states.ack(ack, newest, recount) {
            self.dirty.channel(ack.channel_id);
            self.post_ack_moved(ack.channel_id);
            if !ack.manual {
                self.release(ack.channel_id);
            }
        }
        if ack.manual && self.open.contains(&ack.channel_id) {
            self.hold(ack.channel_id);
        }
    }

    fn hold(&mut self, channel: ChannelId) {
        if !self.held.contains(&channel) {
            self.held.push(channel);
        }
    }

    pub(super) fn release(&mut self, channel: ChannelId) {
        self.held.retain(|held| *held != channel);
    }

    // Module 439147: the position goes to the newest loaded message before `message`, else a
    // millisecond before it, with the loaded messages past it that mention the user.
    pub(crate) fn mark_unread(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Unread> {
        if !self
            .message_actions(channel, message, now_millis)
            .mark_unread
        {
            return None;
        }
        let loaded = self.windows.messages(channel);
        let position = loaded
            .iter()
            .map(|loaded| loaded.id)
            .filter(|id| *id < message)
            .max()
            .unwrap_or_else(|| MessageId::from_unix_millis(message.unix_millis() - 1, 0));
        let mentions = loaded
            .iter()
            .filter(|loaded| loaded.id > position && self.mentions_me(loaded))
            .count();
        let mentions = u32::try_from(mentions).unwrap_or(u32::MAX);
        let before = self.entities.read_states.get(channel);
        self.apply_ack(&MessageAck {
            ack_type: 0,
            channel_id: channel,
            message_id: position.cast(),
            manual: true,
            mention_count: Some(mentions),
            version: 0,
        });
        self.refresh_badges(now_millis, events);
        Some(Unread {
            position,
            mentions,
            before,
            after: self.entities.read_states.get(channel),
        })
    }

    // Mark Unread opens an archived thread and joins an unjoined one first.
    pub(crate) fn unread_needs(
        &self,
        channel: ChannelId,
        message: MessageId,
        now_millis: i64,
    ) -> Option<UnreadNeeds> {
        if !self
            .message_actions(channel, message, now_millis)
            .mark_unread
        {
            return None;
        }
        let thread = self.entities.channels.get(&channel)?.thread.as_deref();
        Some(UnreadNeeds {
            unarchive: thread.is_some_and(|info| info.archived),
            join: thread.is_some_and(|info| !info.joined()),
        })
    }

    pub(crate) fn mentions_me(&self, message: &Message) -> bool {
        let Some(me) = self.entities.me() else {
            return false;
        };
        if message.mention_everyone || message.mentions.iter().any(|user| user.id == me) {
            return true;
        }
        !message.mention_roles.is_empty()
            && self
                .entities
                .channels
                .get(&message.channel_id)
                .and_then(|channel| channel.guild_id)
                .and_then(|guild| self.current_member(guild))
                .is_some_and(|member| {
                    member
                        .roles
                        .iter()
                        .any(|role| message.mention_roles.contains(role))
                })
    }

    // A channel that leaves was left by the user: its hold ends, and it is read no longer.
    pub(crate) fn set_open(&mut self, channels: &[ChannelId]) {
        let before = std::mem::replace(&mut self.open, channels.to_vec());
        let left = |channel: &ChannelId| before.contains(channel) && !channels.contains(channel);
        self.held.retain(|held| channels.contains(held));
        self.reading.retain(|reading| !left(&reading.channel));
    }

    #[cfg(test)]
    pub(crate) fn holds(&self, channel: ChannelId) -> bool {
        self.held.contains(&channel)
    }

    // The connection dropped: nothing is read until the host says so again.
    pub(crate) fn stop_reading(&mut self, events: &mut Vec<StoreEvent>) {
        self.reading.clear();
        self.refresh_badges(self.now_millis(), events);
    }

    // CHANNEL_UNREAD_UPDATE and PASSIVE_UPDATE_V2 come once, when a channel turns unread, so
    // they only ever move a channel's newest message forward, like everything but READY.
    pub(super) fn note_unreads(
        &mut self,
        guild: GuildId,
        channels: &[ChannelUnread],
        events: &mut Vec<StoreEvent>,
    ) {
        for unread in channels {
            // Only a time that is there says something, as in the official client.
            if let Some(pinned) = unread.last_pin_timestamp {
                self.changing_pins(unread.id, events, |state| {
                    if let Some(stored) = state.entities.channels.get_mut(&unread.id)
                        && stored.guild_id == Some(guild)
                        && stored.last_pin_timestamp != Some(pinned)
                    {
                        Arc::make_mut(stored).last_pin_timestamp = Some(pinned);
                    }
                });
            }
            let Some(last) = unread.last_message_id else {
                continue;
            };
            match self.entities.channels.get_mut(&unread.id) {
                Some(stored) if stored.guild_id == Some(guild) => {
                    if Arc::make_mut(stored).note_message(last) {
                        self.dirty.channel(unread.id);
                    }
                    // Sent once, as the channel turns unread: Discord's newest may be past it.
                    self.stale_cursors.insert(unread.id);
                }
                _ => tracing::debug!(
                    channel_id = unread.id.get(),
                    guild_id = guild.get(),
                    "skipping an unread update for an unknown channel"
                ),
            }
        }
    }

    // Discord sends these even from a large guild it sends no other messages of, so the messages
    // before one may be missing.
    pub(super) fn forced_through(&self, message: &model::Message) -> bool {
        let Some(me) = self.entities.me() else {
            return false;
        };
        message.mention_everyone
            || message.mentions.iter().any(|user| user.id == me)
            || (!message.mention_roles.is_empty()
                && self
                    .entities
                    .channels
                    .get(&message.channel_id)
                    .and_then(|channel| channel.guild_id)
                    .and_then(|guild| self.entities.members.get(&guild))
                    .is_some_and(|member| {
                        member
                            .roles
                            .iter()
                            .any(|role| message.mention_roles.contains(role))
                    }))
    }

    // Before the first READY the channels are only remembered; one already read keeps its place,
    // so only its list growing reads it again. Past `READING_LIMIT` channels are ignored.
    pub(crate) fn set_reading(
        &mut self,
        channels: &[ChannelId],
        events: &mut Vec<StoreEvent>,
    ) -> Vec<Ack> {
        let before = std::mem::take(&mut self.reading);
        for &channel in channels {
            if self.reading.len() == READING_LIMIT {
                break;
            }
            if self
                .reading
                .iter()
                .any(|reading| reading.channel == channel)
            {
                continue;
            }
            let kept = before.iter().find(|reading| reading.channel == channel);
            self.reading.push(kept.copied().unwrap_or(Reading {
                channel,
                seen: self.windows.newest_at_present(channel),
                read: false,
            }));
        }
        let mut acks = Vec::new();
        if self.ready {
            for index in 0..self.reading.len() {
                let Reading {
                    channel,
                    seen,
                    read,
                } = self.reading[index];
                if read {
                    continue;
                }
                self.reading[index].read = true;
                if self.held.contains(&channel) {
                    continue;
                }
                if self.is_forum(channel) {
                    acks.extend(self.read_forum(channel));
                } else {
                    acks.extend(seen.and_then(|shown| self.read_shown(channel, shown)));
                }
            }
        }
        self.refresh_badges(self.now_millis(), events);
        acks
    }

    // Only the list growing at its present end reads the channel again. A cursor moved by
    // anything else may be past what the list shows, and an ack from elsewhere moves no row.
    pub(super) fn follow_reading(&mut self) {
        for index in 0..self.reading.len() {
            let Reading { channel, seen, .. } = self.reading[index];
            if self.held.contains(&channel) {
                continue;
            }
            if self.is_forum(channel) {
                if let Some(ack) = self.read_forum(channel) {
                    self.due.retain(|due| due.channel != channel);
                    self.due.push(ack);
                }
                continue;
            }
            let shown = self.windows.newest_at_present(channel);
            if shown == seen {
                continue;
            }
            self.reading[index].seen = shown;
            if let Some(newest) = shown.filter(|_| shown > seen)
                && let Some(ack) = self.read_shown(channel, newest)
            {
                self.due.retain(|due| due.channel != channel);
                self.due.push(ack);
            }
        }
    }

    // A READY reads nothing itself: the lists it leaves are read once they grow, or once the
    // host sets them again.
    pub(super) fn follow_ready(&mut self) {
        // Read elsewhere while Akari was away.
        let entities = &self.entities;
        self.held.retain(|held| {
            entities
                .channels
                .get(held)
                .is_some_and(|channel| entities.needs_ack(channel))
        });
        for index in 0..self.reading.len() {
            let channel = self.reading[index].channel;
            self.reading[index] = Reading {
                channel,
                seen: self.windows.newest_at_present(channel),
                read: false,
            };
        }
    }

    fn is_forum(&self, channel: ChannelId) -> bool {
        self.entities
            .channels
            .get(&channel)
            .is_some_and(|stored| stored.is_forum())
    }

    // The official client acks the forum its page shows at "now" whenever a post is newer than
    // the forum's ack or an active post has unread activity past it (ACK_FORUM_ACTIVE_THREADS).
    fn read_forum(&mut self, forum: ChannelId) -> Option<Ack> {
        if !self.ready {
            return None;
        }
        let stored = self.entities.channels.get(&forum)?;
        if !self.forum_needs_ack(stored) {
            return None;
        }
        self.read_locally(forum)
    }

    fn forum_needs_ack(&self, forum: &Channel) -> bool {
        if self.entities.needs_ack(forum) {
            return true;
        }
        let ack = self.entities.read_position(forum);
        self.active_posts(forum).any(|post| {
            post.last_message_id.is_some_and(|last| last.get() > ack) && self.entities.unread(post)
        })
    }

    // What a forum's ack covers: its newest post and its active posts' newest messages.
    fn forum_newest(&self, forum: &Channel) -> Option<MessageId> {
        let activity = self
            .active_posts(forum)
            .filter_map(|post| post.last_message_id.map(Snowflake::cast));
        forum
            .last_message_id
            .map(Snowflake::cast)
            .into_iter()
            .chain(activity)
            .max()
    }

    fn active_posts<'a>(&'a self, forum: &Channel) -> impl Iterator<Item = &'a Arc<Channel>> {
        self.threads
            .of(forum.id)
            .filter_map(|post| self.entities.channels.get(&post))
            .filter(|post| post.thread.as_deref().is_some_and(|info| !info.archived))
    }

    // Up to the list's newest row and never further, so a channel whose cursor is past it stays
    // unread; never at "now" either.
    fn read_shown(&mut self, channel: ChannelId, shown: MessageId) -> Option<Ack> {
        if !self.ready {
            return None;
        }
        let stored = self.entities.channels.get(&channel)?;
        let body = AckBody::of(stored);
        let before = self.entities.read_states.get(channel);
        // Module 218461 `eM`: a forum post never opened is acked once it shows, unread or not.
        let unopened_post = before.is_none_or(|state| state.estimated)
            && stored
                .parent_id
                .and_then(|parent| self.entities.channels.get(&parent))
                .is_some_and(|parent| parent.is_forum());
        if shown.get() <= self.entities.read_position(stored)
            && before.is_none_or(|state| state.mentions == 0)
            && !unopened_post
        {
            return None;
        }
        self.read_up_to(channel, Some(shown));
        let after = self.entities.read_states.get(channel);
        Some(Ack {
            channel,
            cursor: after.and_then(|state| state.acked),
            now: false,
            before,
            after,
            body,
        })
    }

    // Only while the store holds what the refused ack left: an ack from elsewhere, another local
    // ack or a READY since is newer. A READY being prepared mustn't replay it.
    pub(crate) fn undo_ack(
        &mut self,
        channel: ChannelId,
        undo: Option<ReadState>,
        after: Option<ReadState>,
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        if self.entities.read_states.get(channel) != after {
            return false;
        }
        self.entities.read_states.restore(channel, undo);
        self.release(channel);
        self.post_ack_moved(channel);
        if let Some(journal) = &mut self.journal {
            journal.acks.retain(|(journaled, _)| *journaled != channel);
            journal.undone.insert(channel);
        }
        self.dirty.channel(channel);
        self.refresh_badges(self.now_millis(), events);
        true
    }

    pub(crate) fn take_reading_acks(&mut self) -> Vec<Ack> {
        std::mem::take(&mut self.due)
    }

    // `None` when the channel is read and has no mentions: nothing to send.
    fn ack_locally(&mut self, channel: ChannelId) -> Option<Ack> {
        let stored = self.entities.channels.get(&channel)?;
        let needed = if stored.is_forum() {
            self.forum_needs_ack(stored)
        } else {
            self.entities.needs_ack(stored)
        };
        if !needed {
            return None;
        }
        self.read_locally(channel)
    }

    fn read_locally(&mut self, channel: ChannelId) -> Option<Ack> {
        if self.is_forum(channel) {
            self.find_forum_position(channel, &mut Vec::new());
        }
        let stored = self.entities.channels.get(&channel)?;
        let mut ack = self.ack_of(stored);
        if stored.is_forum() {
            // Discord gets its own clock's "now", never below what the forum shows; the Mac's
            // moves the local counts.
            ack.cursor = self.forum_newest(stored);
            let now = MessageId::from_unix_millis(self.now_millis(), 0);
            self.read_up_to(channel, ack.cursor.max(Some(now)));
        } else if self.entities.needs_ack(stored) {
            self.read_up_to(channel, ack.cursor);
        }
        ack.after = self.entities.read_states.get(channel);
        Some(ack)
    }

    // Every local ack: a READY being prepared gets it too.
    fn read_up_to(&mut self, channel: ChannelId, cursor: Option<MessageId>) {
        self.release(channel);
        if self.entities.read_states.read(channel, cursor) {
            self.dirty.channel(channel);
            self.post_ack_moved(channel);
        }
        let acked = self
            .entities
            .read_states
            .get(channel)
            .and_then(|state| state.acked);
        if let (Some(journal), Some(acked)) = (&mut self.journal, acked) {
            journal.acks.push((channel, acked));
        }
    }

    // Local acks taken while READY was prepared are newer, up to where they read: a mention READY
    // counts past that stays. Their badges moved after the diff, so theirs are made again.
    pub(super) fn replay(
        &self,
        journal: &Journal,
        next: &mut Entities,
        derived: &mut Derived,
        diff: Option<&mut ReadyDiff>,
    ) -> HashMap<ChannelId, MessageId> {
        let mut replayed: BTreeSet<ChannelId> = journal
            .undone
            .iter()
            .filter(|channel| next.channels.contains_key(channel))
            .copied()
            .collect();
        let mut recounts = HashMap::new();
        for &(channel, acked) in &journal.acks {
            let Some(stored) = next.channels.get(&channel) else {
                continue;
            };
            replayed.insert(channel);
            let ready = next.read_states.get(channel);
            if ready
                .and_then(|state| state.acked)
                .is_some_and(|position| acked < position)
            {
                continue;
            }
            let newest = stored.last_message_id.map(Snowflake::cast);
            let mentions = if Some(acked) >= newest {
                0
            } else {
                recounts.insert(channel, acked);
                ready.map_or(0, |state| state.mentions)
            };
            next.read_states.insert(
                channel,
                ReadState {
                    acked: Some(acked),
                    mentions,
                    estimated: false,
                },
            );
        }
        if replayed.is_empty() {
            return recounts;
        }
        let mut dirty = Dirty::default();
        for channel in &replayed {
            dirty.channel(*channel);
        }
        derived.badges.update(
            next,
            &derived.threads,
            dirty,
            self.now_millis(),
            &mut Vec::new(),
        );
        if let Some(diff) = diff {
            self.badges
                .rediff(&derived.badges, &replayed, &mut diff.events);
        }
        recounts
    }

    // The official client acks forums at "now". A stale cursor may be behind Discord's, and so
    // may every cursor before READY, which come from the cache.
    fn ack_of(&self, channel: &Channel) -> Ack {
        Ack {
            channel: channel.id,
            cursor: channel.last_message_id.map(Snowflake::cast),
            now: !self.ready || channel.is_forum() || self.stale_cursors.contains(&channel.id),
            before: self.entities.read_states.get(channel.id),
            after: None,
            body: AckBody::of(channel),
        }
    }

    pub(crate) fn mark_read(
        &mut self,
        channel: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Ack> {
        let ack = self.ack_locally(channel);
        self.refresh_badges(self.now_millis(), events);
        ack
    }

    // As the official client's bulk: what the badges show, listed threads only (never unjoined
    // ones), and every forum the user can view, read or not.
    pub(crate) fn mark_guilds_read(
        &mut self,
        guilds: &[GuildId],
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Vec<Ack> {
        let mut shown = Vec::new();
        let mut forums = Vec::new();
        for channel in self.entities.channels.values() {
            if !channel
                .guild_id
                .is_some_and(|guild| guilds.contains(&guild))
            {
                continue;
            }
            if channel.is_forum() {
                if self
                    .entities
                    .permissions(channel.id, now_millis)
                    .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
                {
                    forums.push(channel.id);
                }
                continue;
            }
            let listed = !channel.is_thread() || self.badges.listed(channel.id);
            let badge = self.badges.channel(channel.id);
            if listed && (badge.unread || badge.mentions > 0) {
                shown.push(channel.id);
            }
        }
        if shown.is_empty() {
            return Vec::new();
        }
        let mut acks: Vec<Ack> = shown
            .into_iter()
            .chain(forums)
            .filter_map(|channel| self.read_locally(channel))
            .collect();
        acks.sort_unstable_by_key(|ack| ack.channel);
        self.refresh_badges(now_millis, events);
        acks
    }

    // A history page that ends at the channel's newest message.
    pub(super) fn reach_present(
        &mut self,
        channel: ChannelId,
        newest: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        self.stale_cursors.remove(&channel);
        if self.note_message(channel, newest, events) {
            self.dirty.channel(channel);
        }
    }

    // A new post in a forum or media channel is its newest "message".
    pub(super) fn note_post(&mut self, parent: Option<ChannelId>, thread: ChannelId) {
        let Some(stored) = parent.and_then(|parent| self.entities.channels.get_mut(&parent)) else {
            return;
        };
        if stored.is_forum() && Arc::make_mut(stored).note_message(thread.cast()) {
            self.dirty.channel(stored.id);
        }
    }

    // Module 218461: the user's own post is opened and acks its forum up to it, without a
    // request. A forum being read is acked at "now" by its reading instead.
    pub(super) fn read_own_post(
        &mut self,
        parent: Option<ChannelId>,
        post: ChannelId,
        owner: Option<UserId>,
    ) {
        let Some(forum) = parent.filter(|parent| self.is_forum(*parent)) else {
            return;
        };
        if owner.is_none() || owner != self.entities.me() {
            return;
        }
        if self.entities.read_states.get(post).is_none() {
            self.read_up_to(post, Some(post.cast()));
        }
        let read = self.ready && self.reading.iter().any(|reading| reading.channel == forum);
        let behind = self
            .entities
            .read_states
            .get(forum)
            .and_then(|state| state.acked)
            .is_none_or(|acked| acked.get() < post.get());
        if !read && behind {
            self.read_up_to(forum, Some(post.cast()));
        }
    }
}
