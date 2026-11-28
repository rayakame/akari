use std::sync::Arc;

use super::types::Message;
use crate::model::{self, EmojiId};

/// A reaction's emoji: a custom one by its `id`, else a Unicode one by its `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReactionEmoji {
    pub id: Option<EmojiId>,
    /// The Unicode emoji itself, or a custom emoji's name; `None` for a deleted custom emoji.
    pub name: Option<Box<str>>,
    pub animated: bool,
}

impl ReactionEmoji {
    /// Whether both stand for the same reaction, as Discord tells them apart: custom emoji by
    /// ID alone, Unicode emoji by the exact string.
    pub fn same(&self, other: &Self) -> bool {
        match (self.id, other.id) {
            (Some(id), other) => other == Some(id),
            (None, Some(_)) => false,
            (None, None) => self.name == other.name,
        }
    }

    pub(crate) fn from_wire(emoji: model::PartialEmoji) -> Option<Self> {
        if emoji.id.is_none() && emoji.name.as_deref().is_none_or(str::is_empty) {
            return None;
        }
        Some(Self {
            id: emoji.id,
            name: emoji.name.map(String::into_boxed_str),
            animated: emoji.animated,
        })
    }
}

/// One emoji's reactions on a message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reaction {
    pub emoji: ReactionEmoji,
    /// How many users reacted normally.
    pub normal: u32,
    /// How many users super-reacted.
    pub burst: u32,
    /// The current user is among `normal`.
    pub me: bool,
    /// The current user is among `burst`.
    pub me_burst: bool,
}

impl Reaction {
    pub(crate) fn from_wire(reaction: model::Reaction) -> Option<Self> {
        let (normal, burst) = match reaction.count_details {
            Some(details) => (details.normal, details.burst),
            None => (reaction.count, 0),
        };
        if normal == 0 && burst == 0 {
            return None;
        }
        Some(Self {
            emoji: ReactionEmoji::from_wire(reaction.emoji)?,
            normal,
            burst,
            me: reaction.me,
            me_burst: reaction.me_burst,
        })
    }
}

pub(crate) fn reactions(reactions: Vec<model::Reaction>) -> Option<Arc<[Reaction]>> {
    shared(
        reactions
            .into_iter()
            .filter_map(Reaction::from_wire)
            .collect(),
    )
}

pub(crate) fn shared(reactions: Vec<Reaction>) -> Option<Arc<[Reaction]>> {
    (!reactions.is_empty()).then(|| reactions.into())
}

/// What a reaction dispatch, or the user's own click, does to a message's reactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReactionChange {
    Add {
        emoji: ReactionEmoji,
        burst: bool,
        mine: bool,
        // A DM's other user reacts with an emoji at most once, so a second add is a repeat.
        only_once: bool,
    },
    Remove {
        emoji: ReactionEmoji,
        burst: bool,
        mine: bool,
    },
    RemoveEmoji(ReactionEmoji),
    RemoveAll,
}

impl ReactionChange {
    // `None`: nothing changes.
    fn apply(&self, reactions: &[Reaction]) -> Option<Vec<Reaction>> {
        let find = |emoji: &ReactionEmoji| {
            reactions
                .iter()
                .position(|reaction| reaction.emoji.same(emoji))
        };
        match self {
            Self::Add {
                emoji,
                burst,
                mine,
                only_once,
            } => {
                let Some(index) = find(emoji) else {
                    let mut next = reactions.to_vec();
                    next.push(Reaction {
                        emoji: emoji.clone(),
                        normal: u32::from(!burst),
                        burst: u32::from(*burst),
                        me: *mine && !burst,
                        me_burst: *mine && *burst,
                    });
                    return Some(next);
                };
                let known = &reactions[index];
                let (count, me) = if *burst {
                    (known.burst, known.me_burst)
                } else {
                    (known.normal, known.me)
                };
                if (*mine && me) || (*only_once && !mine && count > u32::from(me)) {
                    return None;
                }
                let mut next = reactions.to_vec();
                let reaction = &mut next[index];
                let (count, me) = if *burst {
                    (&mut reaction.burst, &mut reaction.me_burst)
                } else {
                    (&mut reaction.normal, &mut reaction.me)
                };
                *count = count.saturating_add(1);
                *me |= mine;
                Some(next)
            }
            Self::Remove { emoji, burst, mine } => {
                let index = find(emoji)?;
                let mut next = reactions.to_vec();
                let reaction = &mut next[index];
                let (count, me) = if *burst {
                    (&mut reaction.burst, &mut reaction.me_burst)
                } else {
                    (&mut reaction.normal, &mut reaction.me)
                };
                if *mine {
                    if !*me {
                        return None;
                    }
                    *me = false;
                    *count = count.saturating_sub(1);
                } else {
                    // Never below the user's own, which only their own removal takes.
                    *count = count.saturating_sub(1).max(u32::from(*me));
                }
                if reaction.normal == 0 && reaction.burst == 0 {
                    next.remove(index);
                }
                (next != reactions).then_some(next)
            }
            Self::RemoveEmoji(emoji) => {
                let index = find(emoji)?;
                let mut next = reactions.to_vec();
                next.remove(index);
                Some(next)
            }
            Self::RemoveAll => (!reactions.is_empty()).then(Vec::new),
        }
    }
}

impl Message {
    /// The message's reactions, in Discord's order; empty when it has none.
    pub fn reactions(&self) -> &[Reaction] {
        self.reactions.as_deref().unwrap_or(&[])
    }

    // `None`: nothing changes.
    pub(crate) fn reacted(&self, changes: &[ReactionChange]) -> Option<Self> {
        let mut next: Option<Vec<Reaction>> = None;
        for change in changes {
            let current = next.as_deref().unwrap_or(self.reactions());
            if let Some(changed) = change.apply(current) {
                next = Some(changed);
            }
        }
        let next = shared(next?);
        (next != self.reactions).then(|| Self {
            reactions: next,
            ..self.clone()
        })
    }
}

pub(crate) fn react(copy: &mut Arc<Message>, changes: &[ReactionChange]) -> bool {
    let Some(next) = copy.reacted(changes) else {
        return false;
    };
    *copy = Arc::new(next);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Snowflake;

    fn unicode(name: &str) -> ReactionEmoji {
        ReactionEmoji {
            id: None,
            name: Some(name.into()),
            animated: false,
        }
    }

    fn custom(id: u64, name: Option<&str>) -> ReactionEmoji {
        ReactionEmoji {
            id: Some(Snowflake::new(id)),
            name: name.map(Into::into),
            animated: false,
        }
    }

    fn reaction(emoji: &str, normal: u32, burst: u32, me: bool, me_burst: bool) -> Reaction {
        Reaction {
            emoji: unicode(emoji),
            normal,
            burst,
            me,
            me_burst,
        }
    }

    fn add(emoji: &str, mine: bool) -> ReactionChange {
        ReactionChange::Add {
            emoji: unicode(emoji),
            burst: false,
            mine,
            only_once: false,
        }
    }

    fn add_burst(emoji: &str, mine: bool) -> ReactionChange {
        ReactionChange::Add {
            emoji: unicode(emoji),
            burst: true,
            mine,
            only_once: false,
        }
    }

    fn remove(emoji: &str, mine: bool) -> ReactionChange {
        ReactionChange::Remove {
            emoji: unicode(emoji),
            burst: false,
            mine,
        }
    }

    fn remove_burst(emoji: &str, mine: bool) -> ReactionChange {
        ReactionChange::Remove {
            emoji: unicode(emoji),
            burst: true,
            mine,
        }
    }

    #[test]
    fn custom_emoji_match_by_id_and_unicode_by_the_exact_string() {
        assert!(custom(1, Some("wave")).same(&custom(1, Some("renamed"))));
        assert!(custom(1, None).same(&custom(1, Some("wave"))));
        assert!(!custom(1, Some("wave")).same(&custom(2, Some("wave"))));
        assert!(!custom(1, Some("👍")).same(&unicode("👍")));
        assert!(!unicode("👍").same(&custom(1, Some("👍"))));
        assert!(unicode("👍").same(&unicode("👍")));
        assert!(!unicode("👍").same(&unicode("👍🏽")));
        assert!(!unicode("❤").same(&unicode("❤\u{fe0f}")));
    }

    #[test]
    fn a_new_emoji_is_appended_and_a_known_one_counts_up_in_place() {
        let known = [reaction("👍", 2, 0, false, false)];

        let appended = add("🔥", false).apply(&known).unwrap();
        let counted = add("👍", false).apply(&appended).unwrap();
        let mine = add_burst("🎉", true).apply(&counted).unwrap();

        assert_eq!(
            mine,
            [
                reaction("👍", 3, 0, false, false),
                reaction("🔥", 1, 0, false, false),
                reaction("🎉", 0, 1, false, true),
            ]
        );
    }

    // The echo of the user's own click finds it applied.
    #[test]
    fn my_add_changes_nothing_when_mine_is_set() {
        let known = [reaction("👍", 2, 1, true, true)];

        assert_eq!(add("👍", true).apply(&known), None);
        assert_eq!(add_burst("👍", true).apply(&known), None);
        assert_eq!(
            add("👍", true).apply(&[reaction("👍", 2, 1, false, true)]),
            Some(vec![reaction("👍", 3, 1, true, true)])
        );
        assert_eq!(
            add_burst("👍", true).apply(&[reaction("👍", 2, 1, true, false)]),
            Some(vec![reaction("👍", 2, 2, true, true)])
        );
    }

    #[test]
    fn my_removal_changes_nothing_when_mine_is_clear() {
        let known = [reaction("👍", 2, 1, false, false)];

        assert_eq!(remove("👍", true).apply(&known), None);
        assert_eq!(remove_burst("👍", true).apply(&known), None);
        assert_eq!(
            remove("👍", true).apply(&[reaction("👍", 2, 1, true, true)]),
            Some(vec![reaction("👍", 1, 1, false, true)])
        );
        assert_eq!(
            remove_burst("👍", true).apply(&[reaction("👍", 2, 1, true, true)]),
            Some(vec![reaction("👍", 2, 0, true, false)])
        );
    }

    #[test]
    fn others_never_touch_mine() {
        let known = [reaction("👍", 2, 2, true, true)];

        assert_eq!(
            add("👍", false).apply(&known),
            Some(vec![reaction("👍", 3, 2, true, true)])
        );
        assert_eq!(
            remove("👍", false).apply(&known),
            Some(vec![reaction("👍", 1, 2, true, true)])
        );
        assert_eq!(
            remove_burst("👍", false).apply(&known),
            Some(vec![reaction("👍", 2, 1, true, true)])
        );
    }

    #[test]
    fn a_count_never_underflows_nor_falls_below_mine() {
        assert_eq!(
            remove("👍", false).apply(&[reaction("👍", 0, 3, false, false)]),
            None
        );
        assert_eq!(
            remove("👍", false).apply(&[reaction("👍", 1, 0, true, false)]),
            None,
            "the one left is the user's own"
        );
        assert_eq!(
            remove_burst("👍", false).apply(&[reaction("👍", 0, 1, false, true)]),
            None
        );
        assert_eq!(remove("🔥", false).apply(&[]), None);
        assert_eq!(remove("🔥", true).apply(&[]), None);
    }

    #[test]
    fn an_emoji_leaves_the_list_when_both_counts_reach_zero() {
        let known = [
            reaction("👍", 1, 1, true, false),
            reaction("🔥", 1, 0, false, false),
        ];

        let normal_gone = remove("👍", true).apply(&known).unwrap();
        assert_eq!(normal_gone[0], reaction("👍", 0, 1, false, false));
        let both_gone = remove_burst("👍", false).apply(&normal_gone).unwrap();
        assert_eq!(both_gone, [reaction("🔥", 1, 0, false, false)]);

        let back = add("👍", false).apply(&both_gone).unwrap();
        assert_eq!(back[1], reaction("👍", 1, 0, false, false), "at the end");
    }

    #[test]
    fn removing_an_emoji_takes_both_counts_and_removing_all_takes_everything() {
        let known = [
            reaction("👍", 4, 2, true, true),
            reaction("🔥", 1, 0, false, false),
        ];

        assert_eq!(
            ReactionChange::RemoveEmoji(unicode("👍")).apply(&known),
            Some(vec![reaction("🔥", 1, 0, false, false)])
        );
        assert_eq!(
            ReactionChange::RemoveEmoji(unicode("🎉")).apply(&known),
            None
        );
        assert_eq!(ReactionChange::RemoveAll.apply(&known), Some(Vec::new()));
        assert_eq!(ReactionChange::RemoveAll.apply(&[]), None);
    }

    #[test]
    fn a_dms_other_user_adds_an_emoji_once() {
        let once = |mine| ReactionChange::Add {
            emoji: unicode("👍"),
            burst: false,
            mine,
            only_once: true,
        };

        assert_eq!(
            once(false).apply(&[reaction("👍", 1, 0, false, false)]),
            None
        );
        assert_eq!(
            once(false).apply(&[reaction("👍", 2, 0, true, false)]),
            None
        );
        assert_eq!(
            once(false).apply(&[reaction("👍", 1, 0, true, false)]),
            Some(vec![reaction("👍", 2, 0, true, false)])
        );
        assert_eq!(
            once(true).apply(&[reaction("👍", 1, 0, false, false)]),
            Some(vec![reaction("👍", 2, 0, true, false)])
        );
        assert_eq!(
            once(false).apply(&[reaction("👍", 0, 1, false, false)]),
            Some(vec![reaction("👍", 1, 1, false, false)]),
            "the other count is another reaction"
        );
    }

    #[test]
    fn a_known_emoji_keeps_the_name_the_message_gave_it() {
        let known = [Reaction {
            emoji: custom(7, Some("wave")),
            normal: 1,
            burst: 0,
            me: false,
            me_burst: false,
        }];
        let unnamed = ReactionChange::Add {
            emoji: custom(7, None),
            burst: false,
            mine: false,
            only_once: false,
        };

        let next = unnamed.apply(&known).unwrap();

        assert_eq!(next[0].emoji, custom(7, Some("wave")));
        assert_eq!(next[0].normal, 2);
    }

    #[test]
    fn a_batch_that_nets_to_nothing_changes_nothing() {
        let author: model::User = serde_json::from_value(serde_json::json!({
            "id": "1", "username": "mira", "discriminator": "0", "avatar": null
        }))
        .unwrap();
        let message = Message {
            reactions: shared(vec![reaction("👍", 1, 0, false, false)]),
            ..Message::pending(
                Snowflake::new(1),
                Snowflake::new(2),
                Arc::new(crate::state::User::from_wire(author)),
                String::new(),
                0,
            )
        };

        assert!(
            message
                .reacted(&[add("👍", false), remove("👍", false)])
                .is_none()
        );
        assert!(
            message
                .reacted(&[add("🔥", true), remove("🔥", true)])
                .is_none()
        );
    }

    #[test]
    fn wire_reactions_keep_both_counts_and_drop_what_counts_nothing() {
        let wire = |json: serde_json::Value| -> model::Reaction {
            serde_json::from_str(&json.to_string()).unwrap()
        };
        let detailed = wire(serde_json::json!({
            "emoji": {"id": null, "name": "👍"}, "count": 3,
            "count_details": {"normal": 2, "burst": 1}, "me": true, "me_burst": false
        }));
        let plain = wire(serde_json::json!({"emoji": {"id": null, "name": "🔥"}, "count": 4}));
        let vote = wire(serde_json::json!({
            "emoji": {"id": "5", "name": ""}, "count": 2,
            "count_details": {"normal": 0, "burst": 0, "vote": 2}, "me_vote": true
        }));
        let nameless = wire(serde_json::json!({"emoji": {"id": null, "name": null}, "count": 1}));
        let empty = wire(serde_json::json!({"emoji": {"id": null, "name": ""}, "count": 1}));

        let kept = reactions(vec![detailed, plain, vote, nameless, empty]).unwrap();

        assert_eq!(
            *kept,
            [
                reaction("👍", 2, 1, true, false),
                reaction("🔥", 4, 0, false, false)
            ]
        );
        assert_eq!(reactions(Vec::new()), None);
    }
}
