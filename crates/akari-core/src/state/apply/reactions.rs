use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

use super::State;
use crate::gateway::{MessageReaction, MessageReactionAddMany};
use crate::model::{ChannelId, ChannelType, MessageId, ReactionType};
use crate::state::events::StoreEvent;
use crate::state::reactions::{self, Reaction, ReactionChange, ReactionEmoji};

static NONE: LazyLock<Arc<[Reaction]>> = LazyLock::new(|| Arc::new([]));

// What Discord has of the user's reactions with one emoji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Mine {
    normal: bool,
    burst: bool,
}

impl Mine {
    fn has(self, burst: bool) -> bool {
        if burst { self.burst } else { self.normal }
    }

    fn set(&mut self, burst: bool, mine: bool) {
        if burst {
            self.burst = mine;
        } else {
            self.normal = mine;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    // In either kind.
    Mine,
    Without { burst: bool },
}

// A request for the user's own reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReactionAsk {
    pub(crate) add: bool,
    pub(crate) burst: bool,
}

// A reaction of the user's own while requests for it run: the store shows the user's wish,
// and the gateway's word about it is kept here until they are over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnReaction {
    channel: ChannelId,
    message: MessageId,
    emoji: ReactionEmoji,
    want: Want,
    discord: Mine,
    // The request on its way, and whether the gateway told of it already.
    asked: Option<(ReactionAsk, bool)>,
    // Answered requests whose echoes are still to come, which then say nothing new.
    owed: VecDeque<ReactionAsk>,
}

impl OwnReaction {
    fn is(&self, channel: ChannelId, message: MessageId, emoji: &ReactionEmoji) -> bool {
        self.channel == channel && self.message == message && self.emoji.same(emoji)
    }

    fn next(&self) -> Option<ReactionAsk> {
        let (add, burst) = match self.want {
            Want::Mine => (true, false),
            Want::Without { burst } => (false, burst),
        };
        let satisfied = if add {
            self.discord.normal || self.discord.burst
        } else {
            !self.discord.has(burst)
        };
        (!satisfied).then_some(ReactionAsk { add, burst })
    }

    // Whether the change is the user's own reaction with the emoji, which the store leaves out.
    fn hear(&mut self, change: &ReactionChange) -> bool {
        match change {
            ReactionChange::Add {
                emoji,
                burst,
                mine: true,
                ..
            } if self.emoji.same(emoji) => {
                if self.emoji.name.is_none() {
                    self.emoji.name.clone_from(&emoji.name);
                }
                self.told(ReactionAsk {
                    add: true,
                    burst: *burst,
                });
                true
            }
            ReactionChange::Remove {
                emoji,
                burst,
                mine: true,
            } if self.emoji.same(emoji) => {
                self.told(ReactionAsk {
                    add: false,
                    burst: *burst,
                });
                true
            }
            ReactionChange::RemoveEmoji(emoji) if self.emoji.same(emoji) => {
                self.cleared();
                false
            }
            ReactionChange::RemoveAll => {
                self.cleared();
                false
            }
            _ => false,
        }
    }

    fn told(&mut self, said: ReactionAsk) {
        if self.owed.front() == Some(&said) {
            self.owed.pop_front();
            return;
        }
        self.owed.clear();
        self.discord.set(said.burst, said.add);
        if let Some((asked, echoed)) = &mut self.asked
            && *asked == said
        {
            *echoed = true;
        }
    }

    fn cleared(&mut self) {
        self.owed.clear();
        self.discord = Mine::default();
        if let Some((asked, echoed)) = &mut self.asked
            && !asked.add
        {
            *echoed = true;
        }
    }

    // Whether Discord refused what would be asked for again.
    fn answered(&mut self, done: bool) -> bool {
        let Some((asked, echoed)) = self.asked.take() else {
            return false;
        };
        // An echo before the answer is followed by everything the gateway said after it.
        if done && !echoed {
            self.discord.set(asked.burst, asked.add);
            self.owed.push_back(asked);
        }
        !done && self.next() == Some(asked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wanted {
    // The message the row shows.
    pub(crate) channel: ChannelId,
    pub(crate) message: MessageId,
    // With the name the message has it under.
    pub(crate) emoji: ReactionEmoji,
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
        asked: &ReactionEmoji,
        want: bool,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Wanted> {
        if asked.id.is_none() && asked.name.as_deref().is_none_or(str::is_empty) {
            return None;
        }
        let actions = self.message_actions(channel, row, now_millis);
        let (shown, message) = self.shown_message(channel, row)?;
        let reactions = self.reactions(channel, row);
        let known = reactions.iter().find(|known| known.emoji.same(asked));
        let allowed = match known {
            Some(_) => actions.toggle_reaction,
            None => actions.react,
        };
        if !allowed {
            return None;
        }
        let had = known.map_or(Mine::default(), |known| Mine {
            normal: known.me,
            burst: known.me_burst,
        });
        let burst = had.burst && !had.normal;
        let mut emoji = known.map_or(asked, |known| &known.emoji).clone();
        if emoji.name.is_none() {
            emoji.name.clone_from(&asked.name);
        }
        let changed = (had.normal || had.burst) != want;
        let wish = if want {
            Want::Mine
        } else {
            Want::Without { burst }
        };
        let open = self
            .reacting
            .iter_mut()
            .find(|own| own.is(shown, message, &emoji));
        match open {
            Some(own) => own.want = wish,
            None if changed => self.reacting.push(OwnReaction {
                channel: shown,
                message,
                emoji: emoji.clone(),
                want: wish,
                discord: had,
                asked: None,
                owed: VecDeque::new(),
            }),
            None => {}
        }
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
        }
        Some(Wanted {
            channel: shown,
            message,
            emoji,
            changed,
        })
    }

    // The request that brings Discord to the user's wish, now on its way; `None` when Discord
    // has it so.
    pub(crate) fn ask_reaction(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
    ) -> Option<ReactionAsk> {
        let own = self
            .reacting
            .iter_mut()
            .find(|own| own.is(channel, message, emoji))?;
        let ask = own.next()?;
        own.asked = Some((ask, false));
        Some(ask)
    }

    // `true`: Discord refused the user's wish as it stands, and asking again would repeat it.
    pub(crate) fn reaction_answered(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        done: bool,
    ) -> bool {
        self.reacting
            .iter_mut()
            .find(|own| own.is(channel, message, emoji))
            .is_some_and(|own| own.answered(done))
    }

    // The requests for the reaction are over: the store shows what Discord has.
    pub(crate) fn settle_reaction(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(index) = self
            .reacting
            .iter()
            .position(|own| own.is(channel, message, emoji))
        else {
            return;
        };
        let own = self.reacting.swap_remove(index);
        let change = |burst: bool| {
            if own.discord.has(burst) {
                ReactionChange::Add {
                    emoji: own.emoji.clone(),
                    burst,
                    mine: true,
                    only_once: false,
                }
            } else {
                ReactionChange::Remove {
                    emoji: own.emoji.clone(),
                    burst,
                    mine: true,
                }
            }
        };
        // What is added goes first: an emoji that keeps a count keeps its place in the row.
        let mut changes = [change(false), change(true)];
        changes.sort_by_key(|change| matches!(change, ReactionChange::Remove { .. }));
        self.apply_reactions(channel, message, &changes, events);
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

    pub(super) fn react(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
        events: &mut Vec<StoreEvent>,
    ) {
        let mut open: Vec<&mut OwnReaction> = self
            .reacting
            .iter_mut()
            .filter(|own| own.channel == channel && own.message == id)
            .collect();
        if open.is_empty() {
            self.apply_reactions(channel, id, changes, events);
            return;
        }
        let shown: Vec<ReactionChange> = changes
            .iter()
            .filter_map(|change| {
                let own = open.iter_mut().any(|slot| slot.hear(change));
                match change {
                    change if !own => Some(change.clone()),
                    ReactionChange::Add { emoji, .. } => Some(ReactionChange::Named(emoji.clone())),
                    _ => None,
                }
            })
            .collect();
        self.apply_reactions(channel, id, &shown, events);
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
