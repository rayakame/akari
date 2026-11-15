use super::*;
use crate::model::EmojiId;

const NOW: i64 = 1_800_000_000_000;

fn unicode(emoji: &str) -> ReactionEmoji {
    ReactionEmoji::unicode(emoji)
}

fn days_ago(days: i64) -> i64 {
    NOW - days * DAY
}

fn note(uses: &mut ReactionUses, emoji: &str, times: usize, at: i64) {
    for _ in 0..times {
        uses.note(&unicode(emoji), at);
    }
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

fn smileys(count: usize) -> Vec<&'static str> {
    crate::emoji::unicode_groups(crate::emoji::EmojiVersion::NEWEST)[0]
        .emojis
        .iter()
        .take(count)
        .map(|emoji| emoji.as_str())
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

// Two uses today beat three three weeks ago; at the same score the later use comes first.
#[test]
fn recent_use_ranks_before_old_use() {
    let mut uses = ReactionUses::default();
    note(&mut uses, "😂", 3, days_ago(20));
    note(&mut uses, "👍", 2, NOW);
    note(&mut uses, "🔥", 2, days_ago(1));

    assert_eq!(names(&uses), ["👍", "🔥", "😂"]);
}

// Each age band on both sides of its edge: 😂 used `times` times `days` ago against 👍 used
// `against` times today.
#[test]
fn each_age_weighs_as_discords_frecency() {
    let cases = [
        (3, 7, 6, "😂"),
        (4, 7, 6, "👍"),
        (15, 5, 3, "😂"),
        (16, 5, 3, "👍"),
        (30, 3, 1, "😂"),
        (31, 3, 1, "👍"),
        (45, 4, 1, "😂"),
        (46, 4, 1, "👍"),
        (80, 11, 1, "😂"),
        (81, 11, 1, "👍"),
    ];
    for (days, times, against, first) in cases {
        let mut uses = ReactionUses::default();
        note(&mut uses, "😂", times, days_ago(days));
        note(&mut uses, "👍", against, NOW);

        assert_eq!(names(&uses)[0], first, "{times}× {days} days ago");
    }
}

// Discord's frecency gives the oldest uses nothing; Akari keeps their count.
#[test]
fn the_oldest_uses_still_rank_by_their_count() {
    let mut uses = ReactionUses::default();
    note(&mut uses, "😂", 5, days_ago(100));
    note(&mut uses, "🔥", 1, days_ago(99));

    assert_eq!(names(&uses), ["😂", "🔥"]);
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
    let all = smileys(43);
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

#[test]
fn a_full_list_drops_the_stalest_not_the_least_used() {
    let mut uses = ReactionUses::default();
    let all = smileys(43);
    note(&mut uses, all[0], 3, days_ago(100));
    for emoji in &all[1..42] {
        note(&mut uses, emoji, 1, NOW);
    }

    note(&mut uses, all[42], 1, NOW);

    let kept = names(&uses);
    assert_eq!(kept.len(), RECENT_REACTIONS);
    assert!(!kept.iter().any(|name| name == all[0]), "{kept:?}");
}

#[test]
fn among_equal_scores_the_older_use_goes() {
    let mut uses = ReactionUses::default();
    let all = smileys(43);
    note(&mut uses, all[0], 1, days_ago(1));
    for emoji in &all[1..42] {
        note(&mut uses, emoji, 1, NOW);
    }

    note(&mut uses, all[42], 1, NOW);

    let kept = names(&uses);
    assert_eq!(kept.len(), RECENT_REACTIONS);
    assert!(!kept.iter().any(|name| name == all[0]), "{kept:?}");
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

// A use made while the cache was read is the newest: a full cached list makes room for it.
#[test]
fn a_full_cached_list_keeps_a_use_made_meanwhile() {
    let all = smileys(43);
    let mut cached = ReactionUses::default();
    for emoji in &all[..42] {
        note(&mut cached, emoji, 10, days_ago(2));
    }
    let mut live = ReactionUses::default();
    note(&mut live, all[42], 1, NOW);

    live.merge(&cached);

    let kept = names(&live);
    assert_eq!(kept.len(), RECENT_REACTIONS);
    assert!(kept.iter().any(|name| name == all[42]), "{kept:?}");
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
