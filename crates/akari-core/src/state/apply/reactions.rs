use std::sync::{Arc, LazyLock};

use super::State;
use crate::gateway::{MessageReaction, MessageReactionAddMany};
use crate::model::{ChannelId, ChannelType, MessageId, ReactionType};
use crate::state::events::StoreEvent;
use crate::state::reactions::{self, Reaction, ReactionChange, ReactionEmoji};

static NONE: LazyLock<Arc<[Reaction]>> = LazyLock::new(|| Arc::new([]));

// A reaction of the user's own whose request is on its way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnReaction {
    channel: ChannelId,
    message: MessageId,
    emoji: ReactionEmoji,
    // What the gateway last said of the user's reaction since Discord last answered.
    heard: Option<bool>,
}

impl OwnReaction {
    fn is(&self, channel: ChannelId, message: MessageId, emoji: &ReactionEmoji) -> bool {
        self.channel == channel && self.message == message && self.emoji.same(emoji)
    }

    fn hear(&mut self, change: &ReactionChange) {
        match change {
            ReactionChange::Add {
                emoji, mine: true, ..
            } if self.emoji.same(emoji) => self.heard = Some(true),
            ReactionChange::Remove {
                emoji, mine: true, ..
            } if self.emoji.same(emoji) => self.heard = Some(false),
            ReactionChange::RemoveEmoji(emoji) if self.emoji.same(emoji) => {
                self.heard = Some(false);
            }
            ReactionChange::RemoveAll => self.heard = Some(false),
            _ => {}
        }
    }
}

/// What the user's click on a reaction changed, and what its request needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wanted {
    // The message the row shows.
    pub(crate) channel: ChannelId,
    pub(crate) message: MessageId,
    // With the name the message has it under.
    pub(crate) emoji: ReactionEmoji,
    // A removal takes the user's super reaction: it was theirs only as one.
    pub(crate) burst: bool,
    // `false`: the store showed it as wanted already.
    pub(crate) changed: bool,
}

impl State {
    pub(crate) fn reactions(&self, channel: ChannelId, id: MessageId) -> Arc<[Reaction]> {
        self.windows
            .shown_by_row(channel, id)
            .or_else(|| self.windows.message(channel, id))
            .or_else(|| self.windows.shown(channel, id))
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
    // The user reacts (`want`) or takes their reaction back: every copy has it at once, as
    // their own dispatch would leave it. `None` where the message's actions don't offer it.
    pub(crate) fn want_reaction(
        &mut self,
        channel: ChannelId,
        row: MessageId,
        emoji: &ReactionEmoji,
        want: bool,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Wanted> {
        if emoji.id.is_none() && emoji.name.as_deref().is_none_or(str::is_empty) {
            return None;
        }
        let actions = self.message_actions(channel, row, now_millis);
        let (shown, message) = self.shown_message(channel, row)?;
        let reactions = self.reactions(channel, row);
        let known = reactions.iter().find(|known| known.emoji.same(emoji));
        let allowed = match known {
            Some(_) => actions.toggle_reaction,
            None => actions.react,
        };
        if !allowed {
            return None;
        }
        let mine = known.is_some_and(|known| known.me || known.me_burst);
        let burst = !want && known.is_some_and(|known| known.me_burst && !known.me);
        let emoji = known.map_or(emoji, |known| &known.emoji).clone();
        let changed = mine != want;
        if changed {
            let change = if want {
                ReactionChange::Add {
                    emoji: emoji.clone(),
                    burst: false,
                    mine: true,
                    only_once: false,
                }
            } else {
                ReactionChange::Remove {
                    emoji: emoji.clone(),
                    burst,
                    mine: true,
                }
            };
            self.apply_reactions(shown, message, &[change], events);
            if !self
                .reacting
                .iter()
                .any(|own| own.is(shown, message, &emoji))
            {
                self.reacting.push(OwnReaction {
                    channel: shown,
                    message,
                    emoji: emoji.clone(),
                    heard: None,
                });
            }
        }
        Some(Wanted {
            channel: shown,
            message,
            emoji,
            burst,
            changed,
        })
    }

    // Discord answered a request for the reaction: what the gateway said before is older.
    pub(crate) fn confirm_reaction(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
    ) {
        if let Some(own) = self
            .reacting
            .iter_mut()
            .find(|own| own.is(channel, message, emoji))
        {
            own.heard = None;
        }
    }

    // The requests for the reaction are over. After a refusal `restore` is what Discord had
    // before (mine or not, as a super reaction or not): the store goes back to it, or to what
    // the gateway said since.
    pub(crate) fn settle_reaction(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        restore: Option<(bool, bool)>,
        events: &mut Vec<StoreEvent>,
    ) {
        let heard = self
            .reacting
            .iter()
            .position(|own| own.is(channel, message, emoji))
            .and_then(|index| self.reacting.swap_remove(index).heard);
        let Some((had, burst)) = restore else {
            return;
        };
        let reactions = self.reactions(channel, message);
        let known = reactions.iter().find(|known| known.emoji.same(emoji));
        let change = if heard.unwrap_or(had) {
            if known.is_some_and(|known| known.me || known.me_burst) {
                return;
            }
            ReactionChange::Add {
                emoji: emoji.clone(),
                burst,
                mine: true,
                only_once: false,
            }
        } else {
            ReactionChange::Remove {
                emoji: emoji.clone(),
                burst: known.is_some_and(|known| known.me_burst && !known.me),
                mine: true,
            }
        };
        self.apply_reactions(channel, message, &[change], events);
    }

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

    // What the gateway says.
    pub(super) fn react(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
        events: &mut Vec<StoreEvent>,
    ) {
        for own in &mut self.reacting {
            if own.channel == channel && own.message == id {
                changes.iter().for_each(|change| own.hear(change));
            }
        }
        self.apply_reactions(channel, id, changes, events);
    }

    // Every copy of the message takes the changes; only what shows one says so.
    fn apply_reactions(
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
