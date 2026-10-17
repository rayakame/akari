use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use super::{Derived, Entities, Journal, ReadyDiff, State, wall_millis};
use crate::gateway::{ChannelUnread, MessageAck};
use crate::model::{
    self, ChannelId, ChannelType, GenericMarker, GuildId, MessageId, MessageType, Permissions,
    Snowflake, Timestamp,
};
use crate::state::badges::Dirty;
use crate::state::events::StoreEvent;
use crate::state::read_states::{Ack, ReadState};
use crate::state::settings::NotificationLevel;
use crate::state::types::Channel;

const MENTION_ON_ALL_MESSAGES: u64 = 1 << 5;

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
                .joined_position(channel)
                .unwrap_or_else(|| channel.last_message_id.map_or(0, Snowflake::get)),
        }
    }

    // Without a read state, what was posted before the user joined is read. DMs, and guilds
    // whose join is unknown, are read until a message arrives.
    fn joined_position(&self, channel: &Channel) -> Option<u64> {
        let joined = self.members.get(&channel.guild_id?)?.joined_at?;
        Some(Snowflake::<GenericMarker>::from_unix_millis(joined.unix_millis(), 0).get())
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
            return;
        };
        let joined = self.entities.joined_position(&channel);
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
            if message.kind != MessageType::PollResult {
                self.entities.read_states.read_up_to(channel.id, message.id);
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
        if self.counts_as_mention(&channel, message, wall_millis()) {
            self.entities.read_states.mention(channel.id, start);
        }
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
        let member = channel.thread.as_deref().is_none_or(|info| info.joined);
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
        }
    }

    // CHANNEL_UNREAD_UPDATE and PASSIVE_UPDATE_V2 come once, when a channel turns unread, so
    // they only ever move a channel's newest message forward, like everything but READY.
    pub(super) fn note_unreads(&mut self, guild: GuildId, channels: &[ChannelUnread]) {
        for unread in channels {
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

    // Before the first READY the channel is only remembered.
    pub(crate) fn set_reading(
        &mut self,
        channel: Option<ChannelId>,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Ack> {
        self.reading = channel.map(|id| (id, self.windows.newest_at_present(id)));
        let ack = self
            .reading
            .and_then(|(id, shown)| self.read_shown(id, shown?));
        self.refresh_badges(wall_millis(), events);
        ack
    }

    // Only the list growing at its present end reads the channel again. A cursor moved by
    // anything else may be past what the list shows, and an ack from elsewhere moves no row.
    pub(super) fn follow_reading(&mut self) {
        let Some((channel, seen)) = self.reading else {
            return;
        };
        let shown = self.windows.newest_at_present(channel);
        if shown == seen {
            return;
        }
        self.reading = Some((channel, shown));
        if let Some(newest) = shown.filter(|_| shown > seen)
            && let Some(ack) = self.read_shown(channel, newest)
        {
            self.due = Some(ack);
        }
    }

    // A READY reads nothing itself: the list it leaves is read once it grows.
    pub(super) fn follow_ready(&mut self) {
        if let Some((channel, _)) = self.reading {
            self.reading = Some((channel, self.windows.newest_at_present(channel)));
        }
    }

    // Up to the list's newest row and never further, so a channel whose cursor is past it stays
    // unread; never at "now" either.
    fn read_shown(&mut self, channel: ChannelId, shown: MessageId) -> Option<Ack> {
        if !self.ready {
            return None;
        }
        let stored = self.entities.channels.get(&channel)?;
        let before = self.entities.read_states.get(channel);
        if shown.get() <= self.entities.read_position(stored)
            && before.is_none_or(|state| state.mentions == 0)
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
        if let Some(journal) = &mut self.journal {
            journal.acks.retain(|(journaled, _)| *journaled != channel);
            journal.undone.insert(channel);
        }
        self.dirty.channel(channel);
        self.refresh_badges(wall_millis(), events);
        true
    }

    pub(crate) fn take_reading_ack(&mut self) -> Option<Ack> {
        self.due.take()
    }

    // `None` when the channel is read and has no mentions: nothing to send.
    fn ack_locally(&mut self, channel: ChannelId) -> Option<Ack> {
        let stored = self.entities.channels.get(&channel)?;
        if !self.entities.needs_ack(stored) {
            return None;
        }
        self.read_locally(channel)
    }

    fn read_locally(&mut self, channel: ChannelId) -> Option<Ack> {
        let stored = self.entities.channels.get(&channel)?;
        let mut ack = self.ack_of(stored);
        if self.entities.needs_ack(stored) {
            self.read_up_to(channel, ack.cursor);
        }
        ack.after = self.entities.read_states.get(channel);
        Some(ack)
    }

    // Every local ack: a READY being prepared gets it too.
    fn read_up_to(&mut self, channel: ChannelId, cursor: Option<MessageId>) {
        if self.entities.read_states.read(channel, cursor) {
            self.dirty.channel(channel);
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
        derived
            .badges
            .update(next, dirty, wall_millis(), &mut Vec::new());
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
            now: !self.ready
                || matches!(
                    channel.kind,
                    ChannelType::GuildForum | ChannelType::GuildMedia
                )
                || self.stale_cursors.contains(&channel.id),
            before: self.entities.read_states.get(channel.id),
            after: None,
        }
    }

    pub(crate) fn mark_read(
        &mut self,
        channel: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Ack> {
        let ack = self.ack_locally(channel);
        self.refresh_badges(wall_millis(), events);
        ack
    }

    // As the official client's bulk: what the badges show, never unjoined threads, and every
    // forum the user can view, read or not.
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
            if matches!(
                channel.kind,
                ChannelType::GuildForum | ChannelType::GuildMedia
            ) {
                if self
                    .entities
                    .permissions(channel.id, now_millis)
                    .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
                {
                    forums.push(channel.id);
                }
                continue;
            }
            let joined = channel
                .thread
                .as_deref()
                .is_none_or(|info| info.joined && !info.archived);
            let badge = self.badges.channel(channel.id);
            if joined && (badge.unread || badge.mentions > 0) {
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
        if matches!(
            stored.kind,
            ChannelType::GuildForum | ChannelType::GuildMedia
        ) && Arc::make_mut(stored).note_message(thread.cast())
        {
            self.dirty.channel(stored.id);
        }
    }
}
