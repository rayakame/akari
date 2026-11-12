use std::sync::{Arc, LazyLock};

use super::State;
use crate::gateway::{MessageReaction, MessageReactionAddMany};
use crate::model::{ChannelId, ChannelType, MessageId, MessageType, ReactionType};
use crate::state::events::StoreEvent;
use crate::state::reactions::{self, Reaction, ReactionChange, ReactionEmoji};
use crate::state::types::StarterMessage;

static NONE: LazyLock<Arc<[Reaction]>> = LazyLock::new(|| Arc::new([]));

impl State {
    pub(crate) fn reactions(&self, channel: ChannelId, id: MessageId) -> Arc<[Reaction]> {
        let row = self.windows.message(channel, id).map(|row| {
            if row.kind != MessageType::ThreadStarterMessage {
                return row;
            }
            match self.windows.starter_message(channel) {
                Some(StarterMessage::Loaded(starter)) => starter,
                _ => row,
            }
        });
        row.or_else(|| self.windows.shown(channel, id))
            .or_else(|| self.preview_message(channel, id))
            .or_else(|| {
                self.latest
                    .get(&channel)
                    .filter(|known| known.id == id)
                    .cloned()
            })
            .and_then(|message| message.reactions.clone())
            .unwrap_or_else(|| NONE.clone())
    }

    pub(crate) fn reactions_of(
        &self,
        channel: ChannelId,
        ids: &[MessageId],
    ) -> Vec<(MessageId, Arc<[Reaction]>)> {
        ids.iter()
            .map(|id| (*id, self.reactions(channel, *id)))
            .filter(|(_, reactions)| !reactions.is_empty())
            .collect()
    }
}

impl State {
    fn reaction_change(&self, reaction: MessageReaction, add: bool) -> Option<ReactionChange> {
        let emoji = ReactionEmoji::from_wire(reaction.emoji)?;
        let burst = reaction.burst || reaction.kind == ReactionType::Burst;
        let mine = self.entities.me() == Some(reaction.user_id);
        Some(if add {
            ReactionChange::Add {
                emoji,
                burst,
                mine,
                only_once: self.is_dm(reaction.channel_id),
            }
        } else {
            ReactionChange::Remove { emoji, burst, mine }
        })
    }

    fn is_dm(&self, channel: ChannelId) -> bool {
        self.entities
            .channels
            .get(&channel)
            .is_some_and(|channel| channel.kind == ChannelType::Dm)
    }

    pub(super) fn reaction_event(
        &mut self,
        reaction: MessageReaction,
        add: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        let (channel, id) = (reaction.channel_id, reaction.message_id);
        if let Some(change) = self.reaction_change(reaction, add) {
            self.react(channel, id, &[change], events);
        }
    }

    pub(super) fn reactions_added(
        &mut self,
        many: MessageReactionAddMany,
        events: &mut Vec<StoreEvent>,
    ) {
        let me = self.entities.me();
        let only_once = self.is_dm(many.channel_id);
        let changes: Vec<ReactionChange> = many
            .reactions
            .into_iter()
            .filter_map(|item| Some((ReactionEmoji::from_wire(item.emoji)?, item.users)))
            .flat_map(|(emoji, users)| {
                users.into_iter().map(move |user| ReactionChange::Add {
                    emoji: emoji.clone(),
                    burst: false,
                    mine: me == Some(user),
                    only_once,
                })
            })
            .collect();
        self.react(many.channel_id, many.message_id, &changes, events);
    }

    // Every copy of the message takes the changes; only what shows one says so.
    pub(super) fn react(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
        events: &mut Vec<StoreEvent>,
    ) {
        if let Some(latest) = self.latest.get_mut(&channel).filter(|known| known.id == id) {
            reactions::react(latest, changes);
        }
        let row = self.windows.react(channel, id, changes, events);
        self.react_to_first_message(channel, id, changes, row, events);
    }
}
