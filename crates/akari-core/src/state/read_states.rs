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
    /// Discord sent no read state, so Akari started one: `acked` is then the user's join or
    /// the message before the first one it saw, not a message the user read.
    pub estimated: bool,
}

// A channel the store just marked read, for the account to tell Discord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ack {
    pub(crate) channel: ChannelId,
    // The channel's newest message, or a forum's newest post.
    pub(crate) cursor: Option<MessageId>,
    // Discord's "now" instead, never below the cursor.
    pub(crate) now: bool,
    // The channel's read state before the store took this ack, to restore if Discord refuses it.
    pub(crate) before: Option<ReadState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReadStates {
    channels: HashMap<ChannelId, ReadState>,
    // The newest message counted per channel, so a repeated MESSAGE_CREATE adds nothing.
    counted: HashMap<ChannelId, MessageId>,
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
                        estimated: false,
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

    pub(crate) fn first_sight(&mut self, channel: ChannelId, message: MessageId) -> bool {
        if self
            .counted
            .get(&channel)
            .is_some_and(|counted| *counted >= message)
        {
            return false;
        }
        self.counted.insert(channel, message);
        true
    }

    pub(crate) fn forget(&mut self, channel: ChannelId) {
        self.channels.remove(&channel);
        self.counted.remove(&channel);
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

    pub(crate) fn ack(&mut self, ack: &MessageAck, newest: Option<u64>) -> bool {
        let position = ack.message_id.get();
        let known = self.get(ack.channel_id);
        // An echo of an ack the store already took mustn't reset a mention counted since; a
        // position the store estimated takes a real one at it.
        let behind = known.is_some_and(|state| {
            state.acked.is_some_and(|acked| {
                if state.estimated {
                    acked.get() > position
                } else {
                    acked.get() >= position
                }
            })
        });
        if !ack.manual && behind {
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
                estimated: false,
            },
        )
    }

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
                estimated: false,
            },
        )
    }

    // Akari's own ack: up to `cursor` (never back), without mentions.
    pub(crate) fn read(&mut self, channel: ChannelId, cursor: Option<MessageId>) -> bool {
        let acked = self.get(channel).and_then(|state| state.acked).max(cursor);
        self.put(
            channel,
            ReadState {
                acked,
                mentions: 0,
                estimated: false,
            },
        )
    }

    pub(crate) fn start(&mut self, channel: ChannelId, acked: Option<MessageId>) {
        self.channels.entry(channel).or_insert(ReadState {
            acked,
            mentions: 0,
            estimated: true,
        });
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
