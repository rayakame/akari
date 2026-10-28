use super::*;
use crate::model::{EmojiId, TagId};
use crate::state::{ForumLayout, ForumSort, ForumTag, TagEmoji, TagMatching};

const FORUM: u64 = 300_000_000_000_000_050;
const POST: u64 = 300_000_000_000_000_051;
const BUG: u64 = 500_000_000_000_000_001;
const STAFF: u64 = 500_000_000_000_000_002;

fn forum() -> Value {
    let mut forum = fixture(include_str!(
        "../../../../tests/fixtures/channel_guild_text.json"
    ));
    forum["id"] = FORUM.to_string().into();
    forum["type"] = 15.into();
    forum["available_tags"] = json!([
        {"id": BUG.to_string(), "name": "Bug", "moderated": false, "emoji_id": null, "emoji_name": "🐛"},
        {"id": STAFF.to_string(), "name": "Staff", "moderated": true, "emoji_id": "600000000000000001", "emoji_name": null},
        {"id": "not a snowflake", "name": "Broken"},
    ]);
    forum["default_reaction_emoji"] = json!({"emoji_id": null, "emoji_name": "👍"});
    forum["default_sort_order"] = 1.into();
    forum["default_forum_layout"] = 2.into();
    forum["default_tag_setting"] = "match_all".into();
    forum["default_thread_rate_limit_per_user"] = 30.into();
    forum["template"] = "Steps:\n".into();
    forum
}

fn post(tags: &[u64]) -> Value {
    let mut post = fixture(include_str!(
        "../../../../tests/fixtures/channel_thread.json"
    ));
    post["id"] = POST.to_string().into();
    post["parent_id"] = FORUM.to_string().into();
    post["applied_tags"] = tags.iter().map(u64::to_string).collect::<Vec<_>>().into();
    post["total_message_sent"] = 7.into();
    post
}

fn forum_settings(state: &State) -> crate::state::ForumSettings {
    state
        .channel(Snowflake::new(FORUM))
        .and_then(|forum| forum.forum.as_deref().cloned())
        .expect("a forum's settings")
}

fn post_info(state: &State) -> crate::state::ThreadInfo {
    state
        .channel(Snowflake::new(POST))
        .and_then(|post| post.thread.as_deref().cloned())
        .expect("a post")
}

// D147: a forum keeps what its posts need; a tag Discord sent malformed is left out.
#[test]
fn a_forum_keeps_its_settings_for_posts() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());

    let settings = forum_settings(&state);
    assert_eq!(
        settings.tags.as_ref(),
        [
            ForumTag {
                id: TagId::new(BUG),
                name: "Bug".into(),
                moderated: false,
                emoji: Some(TagEmoji::Unicode("🐛".into())),
            },
            ForumTag {
                id: TagId::new(STAFF),
                name: "Staff".into(),
                moderated: true,
                emoji: Some(TagEmoji::Custom(EmojiId::new(600_000_000_000_000_001))),
            },
        ]
    );
    assert_eq!(
        settings.default_reaction,
        Some(TagEmoji::Unicode("👍".into()))
    );
    assert_eq!(settings.sort, ForumSort::CreationDate);
    assert_eq!(settings.layout, ForumLayout::Gallery);
    assert_eq!(settings.tag_matching, TagMatching::All);
    assert_eq!(settings.post_slowmode, 30);
    assert_eq!(settings.template.as_deref(), Some("Steps:\n"));
}

// Discord's own defaults when a forum leaves a setting out or sends one Akari doesn't know.
#[test]
fn a_forum_without_settings_takes_discords_defaults() {
    let mut state = ready_state();
    let mut bare = forum();
    for field in [
        "available_tags",
        "default_reaction_emoji",
        "default_sort_order",
        "default_tag_setting",
        "default_thread_rate_limit_per_user",
        "template",
    ] {
        bare.as_object_mut().unwrap().remove(field);
    }
    bare["default_forum_layout"] = 0.into();
    apply(&mut state, "CHANNEL_CREATE", bare);

    let settings = forum_settings(&state);
    assert!(settings.tags.is_empty());
    assert_eq!(settings.default_reaction, None);
    assert_eq!(settings.sort, ForumSort::LatestActivity);
    assert_eq!(
        settings.layout,
        ForumLayout::List,
        "0 is Discord's default, a list"
    );
    assert_eq!(settings.tag_matching, TagMatching::Some);
    assert_eq!(settings.post_slowmode, 0);
    assert_eq!(settings.template, None);

    let mut odd = forum();
    odd["default_sort_order"] = 7.into();
    odd["default_forum_layout"] = 9.into();
    odd["default_tag_setting"] = "sometimes".into();
    odd["default_reaction_emoji"] = json!({"emoji_id": null, "emoji_name": null});
    apply(&mut state, "CHANNEL_UPDATE", odd);
    let settings = forum_settings(&state);
    assert_eq!(settings.sort, ForumSort::LatestActivity);
    assert_eq!(settings.layout, ForumLayout::List);
    assert_eq!(settings.tag_matching, TagMatching::Some);
    assert_eq!(settings.default_reaction, None);
}

#[test]
fn only_forum_and_media_channels_have_forum_settings() {
    let state = ready_state();
    let general = state.channel(Snowflake::new(GENERAL)).expect("general");
    assert!(general.forum.is_none());

    let mut state = ready_state();
    let mut media = forum();
    media["type"] = 16.into();
    apply(&mut state, "CHANNEL_CREATE", media);
    assert_eq!(forum_settings(&state).tags.len(), 2);
}

// A post keeps its tags and Discord's count of every message ever sent in it; an update without
// the tags keeps them.
#[test]
fn a_post_keeps_its_tags_and_its_total() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(&mut state, "THREAD_CREATE", post(&[BUG, STAFF]));

    let info = post_info(&state);
    assert_eq!(
        info.applied_tags.as_ref(),
        [TagId::new(BUG), TagId::new(STAFF)]
    );
    assert_eq!(info.total_message_sent, Some(7));

    apply(&mut state, "THREAD_UPDATE", post(&[STAFF]));
    assert_eq!(post_info(&state).applied_tags.as_ref(), [TagId::new(STAFF)]);

    let mut without = post(&[]);
    without.as_object_mut().unwrap().remove("applied_tags");
    without
        .as_object_mut()
        .unwrap()
        .remove("total_message_sent");
    apply(&mut state, "THREAD_UPDATE", without);
    let info = post_info(&state);
    assert_eq!(info.applied_tags.as_ref(), [TagId::new(STAFF)]);
    assert_eq!(info.total_message_sent, Some(7));
}

// CHANNEL_UPDATE carries the whole channel: its tags replace the forum's; without them the
// forum keeps its own.
#[test]
fn an_update_replaces_the_forums_tags() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut renamed = forum();
    renamed["available_tags"] =
        json!([{"id": BUG.to_string(), "name": "Bugs", "moderated": false}]);
    apply(&mut state, "CHANNEL_UPDATE", renamed);
    let settings = forum_settings(&state);
    assert_eq!(settings.tags.len(), 1);
    assert_eq!(settings.tags[0].name.as_ref(), "Bugs");
    assert_eq!(settings.tags[0].emoji, None);

    let mut without = forum();
    without.as_object_mut().unwrap().remove("available_tags");
    apply(&mut state, "CHANNEL_UPDATE", without);
    assert_eq!(forum_settings(&state).tags.len(), 1);
}
