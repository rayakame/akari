use super::*;
use crate::model::{EmojiId, TagId};
use crate::state::{ForumLayout, ForumSort, ForumTag, ForumView, TagEmoji, TagMatching};

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

// The forum's page (task 168, G-1): pinned first, then by activity or creation, the tags matched
// on the client; who's listed and in what order changes only when the list refreshes.

const PINNED: u64 = 1 << 1;
const OLD: u64 = 300_000_000_000_000_060;
const MIDDLE: u64 = 300_000_000_000_000_061;
const NEWEST: u64 = 300_000_000_000_000_062;
const LATER: u64 = 300_000_000_000_000_063;

fn listed_post(id: u64, last_message: u64, tags: &[u64]) -> Value {
    let mut post = post(tags);
    post["id"] = id.to_string().into();
    post["last_message_id"] = last_message.to_string().into();
    post["owner_id"] = MIRA.to_string().into();
    post
}

fn forum_with_posts() -> State {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    // OLD has the newest message; NEWEST the newest ID; MIDDLE is pinned.
    let mut pinned = listed_post(MIDDLE, 400_000_000_000_000_200, &[BUG]);
    pinned["flags"] = PINNED.into();
    for post in [
        listed_post(OLD, 400_000_000_000_000_900, &[BUG, STAFF]),
        pinned,
        listed_post(NEWEST, 400_000_000_000_000_500, &[]),
    ] {
        apply(&mut state, "THREAD_CREATE", post);
    }
    let mut events = Vec::new();
    state.open_forum(Some(Snowflake::new(FORUM)), &mut events);
    state
}

fn view(sort: ForumSort, tags: &[u64], matching: TagMatching) -> ForumView {
    ForumView {
        sort,
        tags: tags.iter().copied().map(TagId::new).collect(),
        matching,
    }
}

fn active(state: &State, view: &ForumView) -> Vec<u64> {
    state
        .forum_posts(Snowflake::new(FORUM), view)
        .active
        .iter()
        .map(|post| post.id.get())
        .collect()
}

#[test]
fn the_list_is_pinned_first_then_by_activity_or_creation() {
    let state = forum_with_posts();
    let any = |sort| view(sort, &[], TagMatching::Some);

    assert_eq!(
        active(&state, &any(ForumSort::LatestActivity)),
        [MIDDLE, OLD, NEWEST]
    );
    assert_eq!(
        active(&state, &any(ForumSort::CreationDate)),
        [MIDDLE, NEWEST, OLD]
    );
}

// Module 207777: an untagged post drops out of a filter; Match Some wants any of the tags, Match
// All every one.
#[test]
fn the_tags_match_some_or_all() {
    let state = forum_with_posts();

    assert_eq!(
        active(
            &state,
            &view(ForumSort::LatestActivity, &[BUG, STAFF], TagMatching::Some)
        ),
        [MIDDLE, OLD]
    );
    assert_eq!(
        active(
            &state,
            &view(ForumSort::LatestActivity, &[BUG, STAFF], TagMatching::All)
        ),
        [OLD]
    );
}

// Someone else's new post waits behind the pill; activity doesn't move a card; a refresh takes
// both in and starts the count over.
#[test]
fn the_list_stays_until_it_refreshes() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);

    let mut created = listed_post(LATER, 400_000_000_000_001_000, &[]);
    created["newly_created"] = true.into();
    let events = apply(&mut state, "THREAD_CREATE", created);
    assert!(
        events.contains(&format!("ForumPostsChanged({FORUM})")),
        "{events:?}"
    );
    let mut busy = listed_post(NEWEST, 400_000_000_000_002_000, &[]);
    busy.as_object_mut().unwrap().remove("applied_tags");
    apply(&mut state, "THREAD_UPDATE", busy);

    let posts = state.forum_posts(Snowflake::new(FORUM), &any);
    assert_eq!(posts.new_posts, 1);
    assert_eq!(active(&state, &any), [MIDDLE, OLD, NEWEST]);

    let mut events = Vec::new();
    state.refresh_forum(Snowflake::new(FORUM), &mut events);
    assert_eq!(active(&state, &any), [MIDDLE, NEWEST, LATER, OLD]);
    assert_eq!(state.forum_posts(Snowflake::new(FORUM), &any).new_posts, 0);
}

// An existing post the user can now see, and the user's own new post, aren't counted; the user's
// own is listed at once.
#[test]
fn only_others_new_posts_count() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);

    apply(
        &mut state,
        "THREAD_CREATE",
        listed_post(LATER, 400_000_000_000_001_000, &[]),
    );
    assert_eq!(state.forum_posts(Snowflake::new(FORUM), &any).new_posts, 0);

    let mut mine = listed_post(LATER + 1, 400_000_000_000_001_100, &[]);
    mine["owner_id"] = ME.to_string().into();
    mine["newly_created"] = true.into();
    apply(&mut state, "THREAD_CREATE", mine);
    let posts = state.forum_posts(Snowflake::new(FORUM), &any);
    assert_eq!(posts.new_posts, 0);
    assert!(posts.active.iter().any(|post| post.id.get() == LATER + 1));
}

// A deleted post leaves at once; a newly pinned one moves to the top.
#[test]
fn deletions_and_pins_change_the_list_between_refreshes() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);

    apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": OLD.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );
    assert_eq!(active(&state, &any), [MIDDLE, NEWEST]);

    let mut pinned = listed_post(NEWEST, 400_000_000_000_000_500, &[]);
    pinned["flags"] = PINNED.into();
    apply(&mut state, "THREAD_UPDATE", pinned);
    assert_eq!(active(&state, &any)[0], NEWEST);
}

// A new connection or the guild's thread sync refresh the open forum; a closed forum keeps no
// list.
#[test]
fn a_sync_refreshes_the_open_forum() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    let mut created = listed_post(LATER, 400_000_000_000_001_000, &[]);
    created["newly_created"] = true.into();
    apply(&mut state, "THREAD_CREATE", created);

    let sync = json!({
        "guild_id": G1.to_string(),
        "channel_ids": [FORUM.to_string()],
        "threads": [],
        "members": [],
    });
    apply(&mut state, "THREAD_LIST_SYNC", sync);
    let posts = state.forum_posts(Snowflake::new(FORUM), &any);
    assert_eq!(posts.new_posts, 0);

    let mut events = Vec::new();
    state.open_forum(None, &mut events);
    assert_eq!(state.forum_posts(Snowflake::new(FORUM), &any).new_posts, 0);
}
