use std::collections::{HashMap, HashSet};

use crate::gateway::{MessageAck, ReadStateEntry, Relationship, RelationshipUpdate, Versioned};
use crate::model::{ChannelId, MessageId, RelationshipType, Snowflake, UserId};

const CHANNEL: u8 = 0;

/// Where the user stopped reading a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadState {
    /// The last acknowledged message; `None` when nothing is read yet. In forum and media
    /// channels it is a thread.
    pub acked: Option<MessageId>,
    /// Unread mentions, the red count.
    pub mentions: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReadStates {
    channels: HashMap<ChannelId, ReadState>,
    blocked: HashSet<UserId>,
    ignored: HashSet<UserId>,
}

impl ReadStates {
    pub(crate) fn from_ready(
        read_states: Versioned<ReadStateEntry>,
        relationships: Vec<Relationship>,
    ) -> Self {
        let mut next = Self {
            channels: read_states
                .entries
                .into_iter()
                .filter(|entry| entry.read_state_type == CHANNEL)
                .map(|entry| {
                    let state = ReadState {
                        acked: entry.last_message_id.map(Snowflake::cast),
                        mentions: entry.mention_count,
                    };
                    (entry.id.cast(), state)
                })
                .collect(),
            ..Self::default()
        };
        for relationship in relationships {
            next.relate(relationship);
        }
        next
    }

    pub(crate) fn channels(&self) -> &HashMap<ChannelId, ReadState> {
        &self.channels
    }

    pub(crate) fn insert(&mut self, channel: ChannelId, state: ReadState) {
        self.channels.insert(channel, state);
    }

    pub(crate) fn get(&self, channel: ChannelId) -> Option<ReadState> {
        self.channels.get(&channel).copied()
    }

    /// Blocked or ignored: their messages never count as mentions.
    pub(crate) fn ignores(&self, user: UserId) -> bool {
        self.blocked.contains(&user) || self.ignored.contains(&user)
    }

    pub(crate) fn relate(&mut self, relationship: Relationship) {
        let blocked = relationship.kind == RelationshipType::Blocked;
        set(&mut self.blocked, relationship.id, blocked);
        set(
            &mut self.ignored,
            relationship.id,
            relationship.user_ignored,
        );
    }

    pub(crate) fn update_relationship(&mut self, update: RelationshipUpdate) {
        if let Some(ignored) = update.user_ignored {
            set(&mut self.ignored, update.id, ignored);
        }
    }

    pub(crate) fn unrelate(&mut self, user: UserId) {
        self.blocked.remove(&user);
        self.ignored.remove(&user);
    }

    // Forward unless `manual`; without a count, mentions clear once the ack reaches `newest`.
    pub(crate) fn ack(&mut self, ack: &MessageAck, newest: Option<u64>) -> bool {
        let position = ack.message_id.get();
        let known = self.get(ack.channel_id);
        let acked = known.and_then(|state| state.acked);
        if !ack.manual && acked.is_some_and(|acked| acked.get() > position) {
            return false;
        }
        let mentions = match ack.mention_count {
            Some(count) => count,
            None if newest.is_none_or(|newest| position >= newest) => 0,
            None => known.map_or(0, |state| state.mentions),
        };
        self.put(
            ack.channel_id,
            ReadState {
                acked: Some(Snowflake::new(position)),
                mentions,
            },
        )
    }

    // The user's own message: read up to it.
    pub(crate) fn read_up_to(&mut self, channel: ChannelId, message: MessageId) -> bool {
        if self
            .get(channel)
            .and_then(|state| state.acked)
            .is_some_and(|acked| acked >= message)
        {
            return false;
        }
        self.put(
            channel,
            ReadState {
                acked: Some(message),
                mentions: 0,
            },
        )
    }

    // A channel without a read state starts at `acked` when one is first needed.
    pub(crate) fn start(&mut self, channel: ChannelId, acked: Option<MessageId>) {
        self.channels
            .entry(channel)
            .or_insert(ReadState { acked, mentions: 0 });
    }

    pub(crate) fn mention(&mut self, channel: ChannelId, start: Option<MessageId>) {
        self.start(channel, start);
        if let Some(state) = self.channels.get_mut(&channel) {
            state.mentions = state.mentions.saturating_add(1);
        }
    }

    fn put(&mut self, channel: ChannelId, state: ReadState) -> bool {
        self.channels.insert(channel, state) != Some(state)
    }
}

fn set(users: &mut HashSet<UserId>, user: UserId, member: bool) {
    if member {
        users.insert(user);
    } else {
        users.remove(&user);
    }
}
