use std::sync::Arc;

use super::{Entities, State, wall_millis};
use crate::gateway::{ChannelUnread, MessageAck};
use crate::model::{
    self, ChannelId, ChannelType, GenericMarker, GuildId, MessageType, Permissions, Snowflake,
    Timestamp,
};
use crate::state::read_states::ReadState;
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
        if message.id.get() <= self.entities.read_position(&channel) {
            return;
        }
        if Some(message.author.id) == self.entities.me() {
            if message.kind != MessageType::PollResult {
                self.entities.read_states.read_up_to(channel.id, message.id);
            }
            return;
        }
        let joined = self.entities.joined_position(&channel);
        let start = joined
            .map(Snowflake::new)
            .or(channel.last_message_id)
            .map(Snowflake::cast);
        if joined.is_none() {
            self.entities.read_states.start(channel.id, start);
        }
        if self.counts_as_mention(&channel, message, wall_millis()) {
            self.entities.read_states.mention(channel.id, start);
        }
    }

    // The read state topic's rules for what raises a channel's mention count.
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
        let counts = mentions_me
            || self.mentions_through_everyone_or_roles(guild, message)
            || self.counts_every_message(channel, guild, now);
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

    // `MENTION_ON_ALL_MESSAGES`: every message where all messages notify counts, low importance.
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
        if self.entities.read_states.ack(ack, newest) {
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
                }
                _ => tracing::debug!(
                    channel_id = unread.id.get(),
                    guild_id = guild.get(),
                    "skipping an unread update for an unknown channel"
                ),
            }
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
