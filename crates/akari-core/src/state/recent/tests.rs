use super::*;
use crate::model::EmojiId;

const NOW: i64 = 1_800_000_000_000;

fn unicode(emoji: &str) -> ReactionEmoji {
    ReactionEmoji::unicode(emoji)
}

fn days_ago(days: i64) -> i64 {
    NOW - days * DAY
}

fn names(uses: &ReactionUses) -> Vec<String> {
    uses.ranked(NOW)
        .iter()
        .map(|used| match used.emoji.id {
            Some(id) => id.get().to_string(),
            None => used.emoji.name.as_deref().unwrap_or_default().to_owned(),
        })
        .collect()
}

#[test]
fn more_uses_rank_first() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode("👍"), NOW);
    for _ in 0..3 {
        uses.note(&unicode("😂"), NOW);
    }

    assert_eq!(names(&uses), ["😂", "👍"]);
    assert_eq!(uses.ranked(NOW)[0].count, 3);
}

// One use today beats one a week ago; at the same score the later use comes first.
#[test]
fn recent_use_ranks_before_old_use() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode("😂"), days_ago(7));
    uses.note(&unicode("👍"), NOW);
    uses.note(&unicode("🔥"), days_ago(1));

    assert_eq!(names(&uses), ["👍", "🔥", "😂"]);
}

// Discord's weights: a use within 3 days counts 100, within 15 days 70, so two uses ten days ago
// (140) beat one today (100).
#[test]
fn many_old_uses_can_beat_one_new_use() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode("😂"), days_ago(10));
    uses.note(&unicode("😂"), days_ago(10));
    uses.note(&unicode("👍"), NOW);

    assert_eq!(names(&uses), ["😂", "👍"]);
}

#[test]
fn a_use_keeps_the_latest_time() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode("😂"), days_ago(40));
    uses.note(&unicode("😂"), NOW);

    let ranked = uses.ranked(NOW);
    assert_eq!(ranked.len(), 1);
    assert_eq!((ranked[0].count, ranked[0].last_used), (2, NOW));
}

// ❤ without its variation selector is the same emoji to the picker.
#[test]
fn a_unicode_emoji_counts_in_its_qualified_form() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode("❤"), NOW);
    uses.note(&unicode("❤\u{fe0f}"), NOW);

    assert_eq!(names(&uses), ["❤\u{fe0f}"]);
    assert_eq!(uses.ranked(NOW)[0].count, 2);
}

// A custom emoji is the same by its ID, whatever name it came with; the latest name is kept.
#[test]
fn a_custom_emoji_counts_by_its_id() {
    let mut uses = ReactionUses::default();
    let id = EmojiId::new(7);
    uses.note(&ReactionEmoji::custom(id, None, false), NOW);
    uses.note(&ReactionEmoji::custom(id, Some("wave"), false), NOW);

    let ranked = uses.ranked(NOW);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].count, 2);
    assert_eq!(ranked[0].emoji.name.as_deref(), Some("wave"));
}

#[test]
fn a_nameless_unicode_emoji_isnt_counted() {
    let mut uses = ReactionUses::default();
    uses.note(&unicode(""), NOW);

    assert!(uses.ranked(NOW).is_empty());
}

// At most 42; the one just used always stays, and the weakest other goes.
#[test]
fn at_most_42_are_kept_and_the_weakest_goes() {
    let mut uses = ReactionUses::default();
    let all: Vec<&str> = crate::emoji::unicode_groups(crate::emoji::EmojiVersion::NEWEST)[0]
        .emojis
        .iter()
        .take(44)
        .map(|emoji| emoji.as_str())
        .collect();
    for (index, emoji) in all[..42].iter().enumerate() {
        uses.note(&unicode(emoji), NOW);
        if index != 5 {
            uses.note(&unicode(emoji), NOW);
        }
    }
    assert_eq!(uses.ranked(NOW).len(), 42);

    uses.note(&unicode(all[42]), NOW);

    let kept = names(&uses);
    assert_eq!(kept.len(), RECENT_REACTIONS);
    assert!(kept.iter().any(|name| name == all[42]), "the new one stays");
    assert!(!kept.iter().any(|name| name == all[5]), "the weakest goes");
}

// Uses noted before the cache was read add to the cache's.
#[test]
fn merging_adds_counts_and_keeps_the_latest_use() {
    let mut cached = ReactionUses::default();
    cached.note(&unicode("😂"), days_ago(2));
    cached.note(&unicode("😂"), days_ago(2));
    cached.note(&unicode("🔥"), days_ago(2));
    let mut live = ReactionUses::default();
    live.note(&unicode("😂"), NOW);
    live.note(&unicode("👍"), NOW);

    live.merge(&cached);

    let ranked = live.ranked(NOW);
    let found = |name: &str| {
        ranked
            .iter()
            .find(|used| used.emoji.name.as_deref() == Some(name))
            .map(|used| (used.count, used.last_used))
    };
    assert_eq!(found("😂"), Some((3, NOW)));
    assert_eq!(found("🔥"), Some((1, days_ago(2))));
    assert_eq!(found("👍"), Some((1, NOW)));
}

#[test]
fn uses_read_back_from_the_cache_are_kept_whole() {
    let read = ReactionUses::from_uses(vec![
        ReactionUse {
            emoji: unicode("😂"),
            count: 4,
            last_used: days_ago(1),
        },
        ReactionUse {
            emoji: unicode("👍"),
            count: 1,
            last_used: NOW,
        },
    ]);

    assert_eq!(names(&read), ["😂", "👍"]);
    assert_eq!(read.uses().len(), 2);
}
