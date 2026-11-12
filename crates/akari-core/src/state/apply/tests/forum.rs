use super::*;
use crate::model::{ChannelId, MessageId};
use crate::model::{EmojiId, TagId};
use crate::state::apply::wall_millis;
use crate::state::{
    ForumLayout, ForumSort, ForumTag, ForumView, OlderThreads, PostCount, PostPreview, StoreEvent,
    TagEmoji, TagMatching,
};

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

// Post data (task 169, G-4): a card's first message, from post-data, a search answer, the post's
// own messages.

fn starter(post: u64, content: &str) -> Value {
    let mut message = fixture(include_str!(
        "../../../../tests/fixtures/message_create.json"
    ));
    message["id"] = post.to_string().into();
    message["channel_id"] = post.to_string().into();
    message["content"] = content.into();
    message
}

fn preview(state: &State, post: u64) -> Option<String> {
    state
        .post_preview(Snowflake::new(post))
        .map(|preview| match preview {
            PostPreview::Loaded(message) => message.content.to_string(),
            PostPreview::Deleted => "deleted".to_owned(),
        })
}

// The post's first message has the post's ID: it arrives with the post, changes and goes.
#[test]
fn a_posts_first_message_follows_its_own_events() {
    let mut state = forum_with_posts();
    assert_eq!(preview(&state, OLD), None, "not loaded");

    let events = apply(&mut state, "MESSAGE_CREATE", starter(OLD, "how do I…"));
    assert!(
        events.contains(&format!("PostPreviewChanged({OLD})")),
        "{events:?}"
    );
    assert_eq!(preview(&state, OLD).as_deref(), Some("how do I…"));

    apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({"id": OLD.to_string(), "channel_id": OLD.to_string(), "content": "how do I build?"}),
    );
    assert_eq!(preview(&state, OLD).as_deref(), Some("how do I build?"));

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": OLD.to_string(), "channel_id": OLD.to_string()}),
    );
    assert_eq!(preview(&state, OLD).as_deref(), Some("deleted"));
}

// Module 473503: at most 10 a request, all of one forum, each post asked once while it's being
// asked; a post that has its preview isn't asked.
#[test]
fn previews_are_asked_ten_at_a_time() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();
    apply(&mut state, "MESSAGE_CREATE", starter(OLD, "known"));
    let posts: Vec<ChannelId> = (0..15)
        .map(|offset| Snowflake::new(LATER + 10 + offset))
        .chain([Snowflake::new(OLD)])
        .collect();

    assert!(state.want_post_previews(Snowflake::new(FORUM), &posts, &mut events));
    assert!(
        !state.want_post_previews(Snowflake::new(FORUM), &posts, &mut events),
        "already asked"
    );

    let (forum, first) = state.next_post_batch().expect("a batch");
    assert_eq!(forum, Snowflake::new(FORUM));
    assert_eq!(first, posts[..10]);
    let (_, second) = state.next_post_batch().expect("the rest");
    assert_eq!(second, posts[10..15], "OLD has its preview");
    assert_eq!(state.next_post_batch(), None);
}

// What the answer brings fills the previews; a post it leaves out, or a failed request's, is asked
// again only after 30 s.
#[tokio::test(start_paused = true)]
async fn an_answer_fills_previews_and_a_miss_waits() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();
    let asked = [Snowflake::new(OLD), Snowflake::new(NEWEST)];
    state.want_post_previews(Snowflake::new(FORUM), &asked, &mut events);
    let (forum, batch) = state.next_post_batch().expect("a batch");

    let answer: crate::model::PostData = serde_json::from_str(
        &json!({
            "threads": {
                OLD.to_string(): {"first_message": starter(OLD, "first"), "owner": null},
            }
        })
        .to_string(),
    )
    .unwrap();
    events.clear();
    state.finish_post_batch(forum, &batch, Some(answer), &mut events);
    assert_eq!(preview(&state, OLD).as_deref(), Some("first"));
    assert_eq!(preview(&state, NEWEST), None);
    assert!(events.iter().any(|event| matches!(
        event,
        StoreEvent::PostPreviewChanged { post_id } if post_id.get() == OLD
    )));

    assert!(!state.want_post_previews(Snowflake::new(FORUM), &asked, &mut events));
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert!(state.want_post_previews(Snowflake::new(FORUM), &asked, &mut events));
    assert_eq!(
        state.next_post_batch().map(|(_, posts)| posts),
        Some(vec![Snowflake::new(NEWEST)])
    );
}

// Archived pages and searches bring their posts' first messages.
#[test]
fn a_search_brings_first_messages() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();
    let results: crate::model::ThreadSearchResults = serde_json::from_str(
        &json!({
            "threads": [listed_post(LATER, 400_000_000_000_001_000, &[])],
            "members": [],
            "first_messages": [starter(LATER, "found")],
            "has_more": false,
        })
        .to_string(),
    )
    .unwrap();

    let generation = state.generation();
    state.keep_found_threads(results, generation, &mut events);

    assert_eq!(preview(&state, LATER).as_deref(), Some("found"));
}

// The forum's row (task 170, G-8): "N New" for posts newer than the forum's ack that the user
// neither joined nor opened, when its unread setting is All Messages; else the count of posts
// with unread activity; nothing while muted.

const FORUM_ACK: u64 = 300_000_000_000_000_055;

fn read_ack(channel: u64, message: u64) -> Value {
    json!({"channel_id": channel.to_string(), "message_id": message.to_string(), "version": 1})
}

fn forum_with_new_posts() -> State {
    // Without the new notifications every channel's unread setting is All Messages.
    let mut ready = ready_value();
    ready["notification_settings"]["flags"] = 0.into();
    ready["user_guild_settings"]["entries"][0]["flags"] = 0.into();
    let mut state = state_from(&ready);
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(&mut state, "MESSAGE_ACK", read_ack(FORUM, FORUM_ACK));
    // OLD and MIDDLE are newer than the ack; MIDDLE is joined; NEWEST is opened (read).
    for id in [OLD, MIDDLE, NEWEST] {
        let mut post = listed_post(id, id + 1, &[]);
        if id == MIDDLE {
            post["member"] = json!({"id": id.to_string(), "user_id": ME.to_string(), "join_timestamp": "2026-10-01T00:00:00+00:00", "flags": 1});
        }
        apply(&mut state, "THREAD_CREATE", post);
    }
    apply(&mut state, "MESSAGE_ACK", read_ack(NEWEST, NEWEST + 1));
    apply(&mut state, "MESSAGE_ACK", read_ack(MIDDLE, MIDDLE + 1));
    state
}

fn posts_count(state: &State) -> PostCount {
    state.channel_badge(Snowflake::new(FORUM)).posts
}

#[test]
fn the_row_counts_new_posts() {
    let state = forum_with_new_posts();

    assert_eq!(posts_count(&state), PostCount::New(1), "OLD alone");
}

// Opening a post (its read state) or the forum's ack moving past it takes it off the count.
#[test]
fn an_opened_post_is_no_longer_new() {
    let mut state = forum_with_new_posts();

    let events = apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 1));
    assert!(
        events.contains(&format!("BadgeChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(posts_count(&state), PostCount::None);
}

#[test]
fn a_muted_forum_counts_nothing() {
    let mut state = forum_with_new_posts();
    apply(
        &mut state,
        "USER_GUILD_SETTINGS_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "channel_overrides": [{"channel_id": FORUM.to_string(), "muted": true, "message_notifications": 3}],
        }),
    );

    assert_eq!(posts_count(&state), PostCount::None);
}

// With the new notifications and the forum's unreads set to only mentions, the row counts posts
// with unread activity in the muted color instead.
#[test]
fn only_mentions_counts_unread_posts() {
    let mut ready = ready_value();
    ready["notification_settings"]["flags"] = 16.into();
    let mut state = state_from(&ready);
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(&mut state, "MESSAGE_ACK", read_ack(FORUM, FORUM_ACK));
    apply(
        &mut state,
        "THREAD_CREATE",
        started_when_made(listed_post(OLD, OLD + 1, &[])),
    );
    apply(
        &mut state,
        "USER_GUILD_SETTINGS_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "channel_overrides": [{"channel_id": FORUM.to_string(), "muted": false, "message_notifications": 1, "flags": 512}],
        }),
    );

    assert_eq!(posts_count(&state), PostCount::Unread(1));
}

// The forum's page is read like a channel (G-9): reading it acks it at "now" while a post is newer
// than its ack or has unread activity past it, and again as such posts come; locally its ack is
// "now" too, so the row's counts clear.

fn snowflake_at(millis: i64) -> u64 {
    MessageId::from_unix_millis(millis, 0).get()
}

fn read_forum(state: &mut State) -> Vec<crate::state::Ack> {
    state.set_reading(&[Snowflake::new(FORUM)], &mut Vec::new())
}

fn forum_ack(state: &State) -> Option<u64> {
    state
        .read_state(Snowflake::new(FORUM))
        .and_then(|read| read.acked)
        .map(Snowflake::get)
}

#[test]
fn reading_the_forum_acks_it_at_now() {
    let mut state = forum_with_new_posts();
    let newest = state
        .channel(Snowflake::new(FORUM))
        .and_then(|forum| forum.last_message_id)
        .map(Snowflake::cast);
    let before = snowflake_at(wall_millis());

    let acks = read_forum(&mut state);

    assert_eq!(acks.len(), 1, "{acks:?}");
    assert!(acks[0].now);
    assert_eq!(acks[0].cursor, newest, "never below its newest post");
    assert!(forum_ack(&state) >= Some(before), "{:?}", forum_ack(&state));
    assert_eq!(posts_count(&state), PostCount::None);
}

#[test]
fn reading_a_read_forum_acks_nothing() {
    let mut state = forum_with_new_posts();
    read_forum(&mut state);
    state.set_reading(&[], &mut Vec::new());

    assert!(read_forum(&mut state).is_empty());
}

#[test]
fn a_new_post_while_the_forum_is_read_acks_it_again() {
    let mut state = forum_with_new_posts();
    read_forum(&mut state);
    let later = snowflake_at(wall_millis() + 60_000);

    apply(&mut state, "THREAD_CREATE", listed_post(later, later, &[]));

    let acks = state.take_reading_acks();
    assert_eq!(acks.len(), 1, "{acks:?}");
    assert!(acks[0].now);
    assert!(forum_ack(&state) >= Some(later));
}

#[test]
fn activity_in_an_unread_post_while_the_forum_is_read_acks_it_again() {
    let mut state = forum_with_new_posts();
    read_forum(&mut state);
    let later = snowflake_at(wall_millis() + 60_000);

    apply(&mut state, "MESSAGE_CREATE", message(later, OLD));

    assert_eq!(state.take_reading_acks().len(), 1);
    assert!(state.take_reading_acks().is_empty(), "once");
}

#[test]
fn a_forum_not_read_is_not_acked() {
    let mut state = forum_with_new_posts();
    let later = snowflake_at(wall_millis() + 60_000);

    apply(&mut state, "THREAD_CREATE", listed_post(later, later, &[]));

    assert!(state.take_reading_acks().is_empty());
    assert_eq!(forum_ack(&state), Some(FORUM_ACK));
}

// A new connection keeps the open forum's list as it was until its guild's THREAD_LIST_SYNC, which
// drops a post that went while offline and takes in one that came: Akari keeps unjoined posts
// across READY, unconfirmed, where refreshing at READY would leave only the joined ones.
#[test]
fn a_new_connection_keeps_the_list_until_the_sync() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    let mut ready = ready_value();
    if let Some(channels) = ready["guilds"][0]["channels"].as_array_mut() {
        channels.push(forum());
    }

    apply(&mut state, "READY", ready);
    assert_eq!(
        active(&state, &any),
        [MIDDLE, OLD, NEWEST],
        "kept through READY"
    );

    let mut pinned = listed_post(MIDDLE, 400_000_000_000_000_200, &[BUG]);
    pinned["flags"] = PINNED.into();
    let sync = json!({
        "guild_id": G1.to_string(),
        "channel_ids": [FORUM.to_string()],
        "threads": [
            listed_post(OLD, 400_000_000_000_000_900, &[BUG, STAFF]),
            pinned,
            listed_post(LATER, 400_000_000_000_001_000, &[]),
        ],
        "members": [],
    });
    apply(&mut state, "THREAD_LIST_SYNC", sync);
    assert_eq!(active(&state, &any), [MIDDLE, LATER, OLD]);
}

#[test]
fn a_deleted_post_leaves_its_forums_count() {
    let mut state = forum_with_new_posts();
    assert_eq!(posts_count(&state), PostCount::New(1));

    let events = apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": OLD.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );

    assert!(
        events.contains(&format!("BadgeChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(posts_count(&state), PostCount::None);
}

// A guild back from an outage, or sent again, starts its forums' pages again, as its channels'
// Threads browsers do.
#[test]
fn a_guild_create_starts_its_forums_pages_again() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    let key = crate::state::PageKey::new(Snowflake::new(FORUM), &any);
    let mut events = Vec::new();
    let page = state
        .begin_older_posts(key.clone(), &mut events)
        .expect("a first page");
    let results = serde_json::from_str(r#"{"threads": [], "members": [], "has_more": false}"#)
        .expect("search results");
    state.finish_older_posts(&key, page, results, &mut events);
    assert_eq!(
        state.forum_posts(Snowflake::new(FORUM), &any).older,
        OlderThreads::All
    );
    let mut guild = ready_value()["guilds"][0].clone();
    if let Some(channels) = guild["channels"].as_array_mut() {
        channels.push(forum());
    }

    let events = apply(&mut state, "GUILD_CREATE", guild);

    assert_eq!(
        state.forum_posts(Snowflake::new(FORUM), &any).older,
        OlderThreads::NotLoaded
    );
    assert!(
        events.contains(&format!("ForumPostsChanged({FORUM})")),
        "{events:?}"
    );
}

fn archived_post(id: u64, last_message: u64) -> Value {
    let mut post = listed_post(id, last_message, &[]);
    post["thread_metadata"]["archived"] = true.into();
    post
}

// An archived post a GUILD_CREATE kept but didn't list (its messages show) may be gone: only a
// page brings it back.
#[test]
fn an_archived_post_kept_unconfirmed_waits_for_a_page() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    apply(
        &mut state,
        "THREAD_CREATE",
        archived_post(LATER, 400_000_000_000_000_800),
    );
    load_page(
        &mut state,
        LATER,
        crate::state::LoadKind::Latest,
        &[LATER],
        true,
    );
    let mut guild = ready_value()["guilds"][0].clone();
    if let Some(channels) = guild["channels"].as_array_mut() {
        channels.push(forum());
    }
    apply(&mut state, "GUILD_CREATE", guild);
    let key = crate::state::PageKey::new(Snowflake::new(FORUM), &any);
    let mut events = Vec::new();
    let page = state
        .begin_older_posts(key.clone(), &mut events)
        .expect("a first page");
    let other = archived_post(300_000_000_000_000_064, 400_000_000_000_000_700);
    let results = serde_json::from_str(
        &json!({"threads": [other], "members": [], "has_more": true}).to_string(),
    )
    .expect("search results");

    state.finish_older_posts(&key, page, results, &mut events);

    let archived: Vec<u64> = state
        .forum_posts(Snowflake::new(FORUM), &any)
        .archived
        .iter()
        .map(|post| post.id.get())
        .collect();
    assert_eq!(archived, [300_000_000_000_000_064]);
}

// Escape and the menus mark a forum read whose only news is a post's activity.
#[test]
fn marking_a_forum_read_covers_its_posts_activity() {
    let mut state = forum_with_new_posts();
    read_forum(&mut state);
    state.set_reading(&[], &mut Vec::new());
    let later = snowflake_at(wall_millis() + 60_000);
    apply(&mut state, "MESSAGE_CREATE", message(later, OLD));

    let ack = state.mark_read(Snowflake::new(FORUM), &mut Vec::new());

    let ack = ack.expect("an ack");
    assert!(ack.now);
    assert_eq!(
        ack.cursor.map(Snowflake::get),
        Some(later),
        "never below the newest activity"
    );
    assert!(forum_ack(&state) >= Some(later));
}

// A message past the Mac's "now" (its clock behind Discord's) is acked once, not on every write.
#[test]
fn activity_past_the_macs_now_is_acked_once() {
    let mut state = forum_with_new_posts();
    read_forum(&mut state);
    let ahead = snowflake_at(wall_millis() + 3_600_000);
    apply(&mut state, "MESSAGE_CREATE", message(ahead, OLD));
    assert_eq!(state.take_reading_acks().len(), 1);

    apply(
        &mut state,
        "TYPING_START",
        json!({"channel_id": OLD.to_string(), "user_id": MIRA.to_string(), "timestamp": 1}),
    );
    apply(&mut state, "MESSAGE_ACK", read_ack(NEWEST, NEWEST + 2));

    assert!(state.take_reading_acks().is_empty());
}

// The pill refreshes the open forum only; a forum that isn't open lists its posts as they come.
#[test]
fn refreshing_a_forum_that_isnt_open_changes_nothing() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    let mut events = Vec::new();
    state.open_forum(None, &mut events);

    state.refresh_forum(Snowflake::new(FORUM), &mut events);
    let mut created = listed_post(LATER, 400_000_000_000_001_000, &[]);
    created["newly_created"] = true.into();
    apply(&mut state, "THREAD_CREATE", created);

    let posts = state.forum_posts(Snowflake::new(FORUM), &any);
    assert_eq!(posts.new_posts, 0);
    assert_eq!(active(&state, &any), [MIDDLE, LATER, OLD, NEWEST]);
}

// Opening asks for the first 10 cards as the forum shows them first: by its default sort.
#[test]
fn opening_a_forum_names_its_first_cards_by_its_default_sort() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();

    let first = state.open_forum(Some(Snowflake::new(FORUM)), &mut events);

    let first: Vec<u64> = first.iter().map(|post| post.get()).collect();
    assert_eq!(first, [MIDDLE, NEWEST, OLD], "pinned, then by creation");
}

// First messages are kept for the open forum's cards only: a forum that isn't open asks again
// when it opens.
#[test]
fn first_messages_are_kept_for_the_open_forum_only() {
    let mut state = forum_with_posts();
    apply(&mut state, "MESSAGE_CREATE", starter(OLD, "kept"));
    let mut events = Vec::new();

    state.open_forum(Some(Snowflake::new(300_000_000_000_000_099)), &mut events);
    assert_eq!(preview(&state, OLD), None, "another forum opened");
    apply(&mut state, "MESSAGE_CREATE", starter(NEWEST, "not kept"));
    assert_eq!(preview(&state, NEWEST), None, "its forum isn't open");
}

#[test]
fn a_deleted_post_forgets_its_first_message() {
    let mut state = forum_with_posts();
    apply(&mut state, "MESSAGE_CREATE", starter(OLD, "kept"));

    apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": OLD.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );

    assert_eq!(preview(&state, OLD), None);
}

#[test]
fn a_bulk_delete_of_a_first_message_says_it_went() {
    let mut state = forum_with_posts();
    apply(&mut state, "MESSAGE_CREATE", starter(OLD, "kept"));

    let events = apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": [OLD.to_string()], "channel_id": OLD.to_string(), "guild_id": G1.to_string()}),
    );

    assert_eq!(preview(&state, OLD).as_deref(), Some("deleted"));
    assert!(
        events.contains(&format!("PostPreviewChanged({OLD})")),
        "{events:?}"
    );
}

// A deleted archived post of the open forum leaves its page too.
#[test]
fn a_deleted_archived_post_changes_the_open_forums_page() {
    let mut state = forum_with_posts();
    apply(
        &mut state,
        "THREAD_CREATE",
        archived_post(LATER, 400_000_000_000_000_800),
    );

    let events = apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": LATER.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );

    assert!(
        events.contains(&format!("ForumPostsChanged({FORUM})")),
        "{events:?}"
    );
}

// A page given up (the request dropped) can be asked again.
#[test]
fn an_abandoned_first_page_can_be_asked_again() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    let key = crate::state::PageKey::new(Snowflake::new(FORUM), &any);
    let mut events = Vec::new();
    let page = state
        .begin_older_posts(key.clone(), &mut events)
        .expect("a first page");
    assert!(!state.can_load_older_posts(&key), "one at a time");

    state.abandon_older_posts(&key, page, &mut events);

    assert!(state.can_load_older_posts(&key));
    assert_eq!(
        state.forum_posts(Snowflake::new(FORUM), &any).older,
        OlderThreads::NotLoaded
    );
}

// As the official client: the pages of at most 50 filters, the least recently asked dropped.
#[test]
fn the_pages_of_50_filters_are_kept() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();
    let keys: Vec<crate::state::PageKey> = (0..51)
        .map(|tag| {
            crate::state::PageKey::new(
                Snowflake::new(FORUM),
                &view(ForumSort::LatestActivity, &[600 + tag], TagMatching::Some),
            )
        })
        .collect();
    for key in &keys {
        let page = state
            .begin_older_posts(key.clone(), &mut events)
            .expect("a first page");
        let results = serde_json::from_str(r#"{"threads": [], "members": [], "has_more": false}"#)
            .expect("search results");
        state.finish_older_posts(key, page, results, &mut events);
    }

    assert!(
        state.can_load_older_posts(&keys[0]),
        "the first filter's pages went"
    );
    assert!(
        !state.can_load_older_posts(&keys[1]),
        "the second's are all loaded"
    );
    assert!(!state.can_load_older_posts(&keys[50]));
}

// A post pinned while the list is frozen joins it at the top, also one it didn't list.
#[test]
fn a_newly_pinned_post_joins_the_frozen_list() {
    let mut state = forum_with_posts();
    let any = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    apply(
        &mut state,
        "THREAD_CREATE",
        listed_post(LATER, 400_000_000_000_001_000, &[]),
    );
    assert_eq!(active(&state, &any), [MIDDLE, OLD, NEWEST], "frozen");

    let mut pinned = listed_post(LATER, 400_000_000_000_001_000, &[]);
    pinned["flags"] = PINNED.into();
    apply(&mut state, "THREAD_UPDATE", pinned);

    assert_eq!(active(&state, &any), [LATER, MIDDLE, OLD, NEWEST]);
}

// A post's starter message has the post's own ID: alone, it makes the post unread.
#[test]
fn a_post_with_only_its_starter_is_unread() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut starter_only = started_when_made(post(&[]));
    starter_only["last_message_id"] = POST.to_string().into();

    apply(&mut state, "THREAD_CREATE", starter_only);

    assert!(state.unread(Snowflake::new(POST)));
}

// Discord's start for a post without a read state isn't bound by the guild join, as other
// threads' is (the fixture's member joined G1 at 2024-01-01T12:00:00Z).
#[test]
fn a_post_from_before_the_guild_join_is_unread() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut early = post(&[]);
    early["thread_metadata"]["archive_timestamp"] = "2023-06-01T00:00:00.000000+00:00".into();
    let before_join = Snowflake::<model::GenericMarker>::from_unix_millis(1_704_067_200_000, 0);
    early["last_message_id"] = before_join.get().to_string().into();

    apply(&mut state, "THREAD_CREATE", early);

    assert!(state.unread(Snowflake::new(POST)));
}

// A card's unread look, as Discord's `isForumPostUnread`: an active post with messages past where
// the user stopped reading, whatever its mute or unread setting; an archived post never.
#[test]
fn unread_posts_are_active_ones_with_messages_past_the_read_position() {
    let mut ready = ready_value();
    ready["notification_settings"]["flags"] = 16.into();
    let mut state = state_from(&ready);
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut archived = listed_post(NEWEST, 400_000_000_000_000_500, &[]);
    archived["thread_metadata"]["archived"] = true.into();
    for post in [
        listed_post(OLD, 400_000_000_000_000_900, &[]),
        listed_post(MIDDLE, 400_000_000_000_000_200, &[]),
        archived,
    ] {
        apply(&mut state, "THREAD_CREATE", started_when_made(post));
    }
    apply(
        &mut state,
        "MESSAGE_ACK",
        read_ack(MIDDLE, 400_000_000_000_000_200),
    );
    // Only mentions: the posts show no unread badge, their cards still do.
    apply(
        &mut state,
        "USER_GUILD_SETTINGS_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "channel_overrides": [{"channel_id": FORUM.to_string(), "muted": false, "message_notifications": 1, "flags": 512}],
        }),
    );
    assert!(!state.channel_badge(Snowflake::new(OLD)).unread);

    let unread = state.unread_posts(&[OLD, MIDDLE, NEWEST, LATER].map(Snowflake::new));

    assert_eq!(unread, [Snowflake::new(OLD)]);
}

// Reading a post, even one the user never joined, says to read its card's look again.
#[test]
fn an_ack_on_a_post_says_its_badge_changed() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(
        &mut state,
        "THREAD_CREATE",
        started_when_made(listed_post(OLD, 400_000_000_000_000_900, &[])),
    );
    assert_eq!(state.unread_posts(&[Snowflake::new(OLD)]).len(), 1);

    let events = apply(
        &mut state,
        "MESSAGE_ACK",
        read_ack(OLD, 400_000_000_000_000_900),
    );

    assert!(
        events
            .iter()
            .any(|event| event.starts_with(&format!("BadgeChanged({OLD}"))),
        "{events:?}"
    );
    assert!(state.unread_posts(&[Snowflake::new(OLD)]).is_empty());
}

// A post archived before Discord's epoch starts at the epoch, never below it.
#[test]
fn a_post_archived_before_discords_epoch_is_unread_after_it() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut early = started_when_made(post(&[]));
    early["thread_metadata"]["archive_timestamp"] = "2014-06-01T00:00:00.000000+00:00".into();
    early["last_message_id"] = POST.to_string().into();

    apply(&mut state, "THREAD_CREATE", early);

    assert!(state.unread(Snowflake::new(POST)));
}

// Without its metadata a post is still a thread: unread from its start, as Discord's.
#[test]
fn a_post_without_its_metadata_is_unread_from_its_start() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut bare = post(&[]);
    bare.as_object_mut().unwrap().remove("thread_metadata");
    bare["last_message_id"] = POST.to_string().into();

    apply(&mut state, "THREAD_CREATE", bare);

    assert!(
        state
            .channel(Snowflake::new(POST))
            .is_some_and(|post| post.thread.is_none())
    );
    assert!(state.unread(Snowflake::new(POST)));
}

// CodeRabbit on #61: a card's media resolves through its first message, which post-data can bring
// without the post's own messages.
#[test]
fn a_posts_first_message_from_its_preview_is_a_message_too() {
    let mut state = forum_with_posts();

    apply(&mut state, "MESSAGE_CREATE", starter(OLD, "with a picture"));

    let found = state.message(Snowflake::new(OLD), Snowflake::new(OLD));
    assert_eq!(
        found.map(|message| message.content.to_string()).as_deref(),
        Some("with a picture")
    );
    assert!(
        state
            .message(Snowflake::new(OLD), Snowflake::new(OLD + 1))
            .is_none()
    );
}

// CodeRabbit on #61: the open forum's frozen list keeps its order, not posts the user can no
// longer see.
#[test]
fn the_frozen_list_drops_posts_the_user_can_no_longer_see() {
    let mut ready = ready_value();
    ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
    ready["merged_members"][0][0]["roles"] = json!([]);
    let mut state = state_from(&ready);
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(
        &mut state,
        "THREAD_CREATE",
        listed_post(OLD, 400_000_000_000_000_900, &[]),
    );
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
    let view = view(ForumSort::LatestActivity, &[], TagMatching::Some);
    assert_eq!(
        state.forum_posts(Snowflake::new(FORUM), &view).active.len(),
        1
    );

    apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({
            "id": FORUM.to_string(),
            "permission_overwrites": [{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]
        }),
    );

    assert!(
        state
            .forum_posts(Snowflake::new(FORUM), &view)
            .active
            .is_empty()
    );
}

// A card's NEW badge, as Discord's `isNewForumThread`: an active post the user never opened,
// made since the forum was read before this visit and since the user joined the server.

// The fixture's member joined G1 at 2024-01-01T12:00:00Z.
const JOINED: i64 = 1_704_110_400_000;

fn minutes_after_join(minutes: i64) -> u64 {
    snowflake_at(JOINED + minutes * 60_000)
}

fn post_made(minutes: i64) -> Value {
    let id = minutes_after_join(minutes);
    started_when_made(listed_post(id, id, &[]))
}

fn new_cards(state: &State, minutes: &[i64]) -> Vec<i64> {
    let posts: Vec<ChannelId> = minutes
        .iter()
        .map(|at| Snowflake::new(minutes_after_join(*at)))
        .collect();
    let new = state.new_posts(&posts);
    minutes
        .iter()
        .copied()
        .filter(|at| new.contains(&Snowflake::new(minutes_after_join(*at))))
        .collect()
}

fn open(state: &mut State) {
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
}

// The forum read up to 10 minutes after the join; posts made 5, 20, 21 (joined), 22 (opened),
// 23 (the user's own) and 24 (archived) minutes after it.
fn forum_read_before() -> State {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(
        &mut state,
        "MESSAGE_ACK",
        read_ack(FORUM, minutes_after_join(10)),
    );
    let mut joined = post_made(21);
    joined["member"] = json!({"id": joined["id"], "user_id": ME.to_string(), "join_timestamp": "2026-10-01T00:00:00+00:00", "flags": 1});
    let mut mine = post_made(23);
    mine["owner_id"] = ME.to_string().into();
    let mut archived = post_made(24);
    archived["thread_metadata"]["archived"] = true.into();
    for post in [
        post_made(5),
        post_made(20),
        joined,
        post_made(22),
        mine,
        archived,
    ] {
        apply(&mut state, "THREAD_CREATE", post);
    }
    let opened = minutes_after_join(22);
    apply(&mut state, "MESSAGE_ACK", read_ack(opened, opened));
    state
}

const ALL: [i64; 6] = [5, 20, 21, 22, 23, 24];

#[test]
fn posts_made_since_the_forum_was_last_read_are_new() {
    let mut state = forum_read_before();
    assert_eq!(new_cards(&state, &ALL), [0i64; 0], "no forum is open");

    open(&mut state);

    assert_eq!(new_cards(&state, &ALL), [20, 21], "joined or not");
}

// The forum's own ack moves to "now" while its page shows; the cards stay new until the forum is
// opened the next time.
#[test]
fn the_cards_stay_new_while_the_forum_stays_open() {
    let mut state = forum_read_before();
    open(&mut state);

    read_forum(&mut state);
    state.refresh_forum(Snowflake::new(FORUM), &mut Vec::new());
    assert_eq!(new_cards(&state, &ALL), [20, 21]);

    state.open_forum(None, &mut Vec::new());
    assert_eq!(new_cards(&state, &ALL), [0i64; 0]);
    open(&mut state);
    assert_eq!(new_cards(&state, &ALL), [0i64; 0], "read on the last visit");
}

#[test]
fn a_post_made_while_the_forum_is_open_is_new_unless_its_the_users() {
    let mut state = forum_read_before();
    open(&mut state);
    read_forum(&mut state);

    apply(&mut state, "THREAD_CREATE", post_made(30));
    let mut mine = post_made(31);
    mine["owner_id"] = ME.to_string().into();
    apply(&mut state, "THREAD_CREATE", mine);

    assert_eq!(new_cards(&state, &[30, 31]), [30]);
}

#[test]
fn opening_a_post_or_archiving_it_ends_its_new() {
    let mut state = forum_read_before();
    open(&mut state);

    let read = minutes_after_join(20);
    let events = apply(&mut state, "MESSAGE_ACK", read_ack(read, read));
    assert!(
        events.contains(&format!("BadgeChanged({read})")),
        "{events:?}"
    );
    let mut archived = post_made(21);
    archived["thread_metadata"]["archived"] = true.into();
    apply(&mut state, "THREAD_UPDATE", archived);

    assert_eq!(new_cards(&state, &ALL), [0i64; 0]);
}

// A forum never read counts from the user's join to the server.
#[test]
fn without_a_forum_ack_posts_since_the_join_are_new() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    for minutes in [-5, 5] {
        apply(&mut state, "THREAD_CREATE", post_made(minutes));
    }

    open(&mut state);

    assert_eq!(new_cards(&state, &[-5, 5]), [5]);
}

// A forum read before the user's join (an ack an import left) still counts from the join.
#[test]
fn a_post_from_before_the_join_is_never_new() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(
        &mut state,
        "MESSAGE_ACK",
        read_ack(FORUM, minutes_after_join(-60)),
    );
    for minutes in [-5, 5] {
        apply(&mut state, "THREAD_CREATE", post_made(minutes));
    }

    open(&mut state);

    assert_eq!(new_cards(&state, &[-5, 5]), [5]);
}

// A new connection's thread sync refreshes the list and keeps what the visit began with.
#[test]
fn a_sync_keeps_the_cards_new() {
    let mut state = forum_read_before();
    open(&mut state);
    read_forum(&mut state);

    state.refresh_open_forum(Snowflake::new(G1), &mut Vec::new());

    assert_eq!(new_cards(&state, &ALL), [20, 21]);
}

// A post made 40 minutes after the join whose only message is its starter: nothing in it is
// unread (Discord's start for a never-opened post is its archive timestamp).
fn read_new_post(state: &mut State) -> u64 {
    let id = minutes_after_join(40);
    let mut post = listed_post(id, id, &[]);
    post["thread_metadata"]["archive_timestamp"] = "2024-01-01T12:41:00.000000+00:00".into();
    apply(state, "THREAD_CREATE", post);
    assert!(!state.unread(Snowflake::new(id)));
    id
}

// Discord acks a never-opened post the moment it shows, also with nothing unread in it.
#[test]
fn opening_a_new_post_with_nothing_unread_ends_its_new() {
    let mut state = forum_read_before();
    let post = read_new_post(&mut state);
    open(&mut state);
    assert_eq!(new_cards(&state, &[40]), [40]);
    load_page(&mut state, post, LoadKind::Latest, &[post], true);

    let mut events = Vec::new();
    let acks = state.set_reading(&[Snowflake::new(post)], &mut events);

    assert_eq!(acks.len(), 1, "{acks:?}");
    assert_eq!(acks[0].cursor.map(Snowflake::get), Some(post));
    assert_eq!(new_cards(&state, &[40]), [0i64; 0]);
    assert!(
        events
            .iter()
            .map(describe)
            .any(|event| event == format!("BadgeChanged({post})")),
        "{events:?}"
    );
    state.set_reading(&[], &mut Vec::new());
    assert!(
        state
            .set_reading(&[Snowflake::new(post)], &mut Vec::new())
            .is_empty(),
        "once"
    );
}

// Each time the host opens the forum is a visit of its own, as each selection is in Discord.
#[test]
fn opening_the_open_forum_again_begins_a_new_visit() {
    let mut state = forum_read_before();
    open(&mut state);
    read_forum(&mut state);

    open(&mut state);

    assert_eq!(new_cards(&state, &ALL), [0i64; 0]);
}

#[test]
fn another_forums_posts_and_ones_at_the_position_arent_new() {
    let mut state = forum_read_before();
    let mut other = forum();
    other["id"] = (FORUM + 500).to_string().into();
    apply(&mut state, "CHANNEL_CREATE", other);
    let mut elsewhere = post_made(26);
    elsewhere["parent_id"] = (FORUM + 500).to_string().into();
    apply(&mut state, "THREAD_CREATE", elsewhere);
    apply(&mut state, "THREAD_CREATE", post_made(10));

    open(&mut state);
    assert_eq!(new_cards(&state, &[10, 26]), [0i64; 0]);

    state.open_forum(Some(Snowflake::new(FORUM + 500)), &mut Vec::new());
    assert_eq!(
        new_cards(&state, &[20, 26]),
        [26],
        "its own posts, from the join"
    );
}

// Discord compares times: a post made in the millisecond of the join isn't after it.
#[test]
fn a_post_made_in_the_joins_millisecond_isnt_new() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    let same = snowflake_at(JOINED) + 7;
    apply(
        &mut state,
        "THREAD_CREATE",
        started_when_made(listed_post(same, same, &[])),
    );
    apply(&mut state, "THREAD_CREATE", post_made(1));

    open(&mut state);

    let posts = [same, minutes_after_join(1)].map(Snowflake::new);
    assert_eq!(state.new_posts(&posts), [posts[1]]);
}

// A mention in a post never opened leaves an estimate, not a read state: it's still new.
#[test]
fn a_mention_in_a_post_never_opened_keeps_it_new() {
    let mut state = forum_read_before();
    open(&mut state);
    let post = minutes_after_join(20);
    let mut message = starter(post, "look");
    message["id"] = (post + 5).to_string().into();
    message["mentions"] =
        json!([{"id": ME.to_string(), "username": "me", "discriminator": "0", "avatar": null}]);

    apply(&mut state, "MESSAGE_CREATE", message);

    assert_eq!(new_cards(&state, &[20]), [20]);
}

// The app shows the forum from the disk cache before READY: the first READY's ack is the one
// the visit began with (another device may have read the forum since).
#[test]
fn the_first_ready_after_a_restore_takes_the_position_again() {
    let mut ready = ready_value();
    ready["read_state"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": FORUM.to_string(), "last_message_id": minutes_after_join(21).to_string(), "mention_count": 0, "flags": 0}));
    let mut state = State::new();
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
    assert_eq!(new_cards(&state, &ALL), [0i64; 0], "nothing known yet");

    let events = apply(&mut state, "READY", ready);
    apply(&mut state, "CHANNEL_CREATE", forum());
    for minutes in [20, 30] {
        apply(&mut state, "THREAD_CREATE", post_made(minutes));
    }

    assert!(
        events.contains(&format!("ForumPostsChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(new_cards(&state, &[20, 30]), [30]);

    // A later READY (a new session after a lost one) keeps the visit's position.
    read_forum(&mut state);
    apply(&mut state, "READY", ready_value());
    apply(&mut state, "CHANNEL_CREATE", forum());
    apply(&mut state, "THREAD_CREATE", post_made(30));
    assert_eq!(new_cards(&state, &[30]), [30]);
}

// CodeRabbit on #61: the forum is opened before the store has it (the cache had none, READY has
// no read state for it): its position is taken when it comes, before it's read.
#[test]
fn a_forum_that_comes_after_it_was_opened_takes_its_position_then() {
    let mut state = State::new();
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
    apply(&mut state, "READY", ready_value());
    assert_eq!(new_cards(&state, &[5]), [0i64; 0]);

    let events = apply(&mut state, "CHANNEL_CREATE", forum());
    apply(&mut state, "THREAD_CREATE", post_made(5));

    assert!(
        events.contains(&format!("ForumPostsChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(new_cards(&state, &[5]), [5]);
    read_forum(&mut state);
    assert_eq!(
        new_cards(&state, &[5]),
        [5],
        "the visit's own ack changes nothing"
    );
}

// CodeRabbit on #61: a first message deleted or edited while post-data is on its way stays as
// the gateway left it; the answer is older.
#[test]
fn an_answer_doesnt_undo_what_the_gateway_brought_meanwhile() {
    let mut state = forum_with_posts();
    let mut events = Vec::new();
    let asked = [Snowflake::new(OLD), Snowflake::new(NEWEST)];
    state.want_post_previews(Snowflake::new(FORUM), &asked, &mut events);
    let (forum, batch) = state.next_post_batch().expect("a batch");
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": OLD.to_string(), "channel_id": OLD.to_string()}),
    );
    apply(&mut state, "MESSAGE_CREATE", starter(NEWEST, "live"));

    let answer: crate::model::PostData = serde_json::from_str(
        &json!({
            "threads": {
                OLD.to_string(): {"first_message": starter(OLD, "stale"), "owner": null},
                NEWEST.to_string(): {"first_message": starter(NEWEST, "stale"), "owner": null},
            }
        })
        .to_string(),
    )
    .unwrap();
    events.clear();
    state.finish_post_batch(forum, &batch, Some(answer), &mut events);

    assert_eq!(preview(&state, OLD).as_deref(), Some("deleted"));
    assert_eq!(preview(&state, NEWEST).as_deref(), Some("live"));
    assert!(events.is_empty(), "{events:?}");
    assert!(!state.want_post_previews(Snowflake::new(FORUM), &asked, &mut events));
}

// CodeRabbit on #61: an edit of a first message that comes while its preview is being asked for
// says the answer may be older than it: an older answer is asked for again, a newer one stays.

fn edit_at(post: u64, content: &str, time: &str) -> Value {
    json!({"id": post.to_string(), "channel_id": post.to_string(), "content": content,
           "edited_timestamp": time})
}

fn answered(post: u64, content: &str, edited: Option<&str>) -> crate::model::PostData {
    let mut first = starter(post, content);
    first["edited_timestamp"] = edited.into();
    serde_json::from_str(
        &json!({"threads": {post.to_string(): {"first_message": first, "owner": null}}})
            .to_string(),
    )
    .unwrap()
}

fn asking(state: &mut State, post: u64) -> (ChannelId, Vec<ChannelId>) {
    state.want_post_previews(
        Snowflake::new(FORUM),
        &[Snowflake::new(post)],
        &mut Vec::new(),
    );
    state.next_post_batch().expect("a batch")
}

const NOON: &str = "2026-11-11T12:00:00.000000+00:00";
const LATER_THAT_DAY: &str = "2026-11-11T13:00:00.000000+00:00";

#[test]
fn an_answer_from_before_an_edit_is_asked_for_again() {
    let mut state = forum_with_posts();
    let (forum, batch) = asking(&mut state, OLD);
    apply(&mut state, "MESSAGE_UPDATE", edit_at(OLD, "edited", NOON));
    assert_eq!(preview(&state, OLD), None, "nothing to edit yet");

    let mut events = Vec::new();
    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(OLD, "before the edit", None)),
        &mut events,
    );

    assert_eq!(preview(&state, OLD), None, "the stale answer isn't shown");
    assert!(events.is_empty(), "{events:?}");
    let (forum, batch) = state.next_post_batch().expect("asked again at once");
    assert_eq!(batch, [Snowflake::new(OLD)]);
    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(OLD, "edited", Some(NOON))),
        &mut events,
    );
    assert_eq!(preview(&state, OLD).as_deref(), Some("edited"));
    assert_eq!(state.next_post_batch(), None);
}

#[test]
fn an_answer_newer_than_the_edit_stays() {
    let mut state = forum_with_posts();
    let (forum, batch) = asking(&mut state, OLD);
    apply(
        &mut state,
        "MESSAGE_UPDATE",
        edit_at(OLD, "the first edit", NOON),
    );

    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(OLD, "the second edit", Some(LATER_THAT_DAY))),
        &mut Vec::new(),
    );

    assert_eq!(preview(&state, OLD).as_deref(), Some("the second edit"));
    assert_eq!(state.next_post_batch(), None);
}

// An update that says no edit time (Discord filling in a link's embed) could be either side of
// the answer: asked again, once.
#[test]
fn an_update_without_a_time_asks_once_more() {
    let mut state = forum_with_posts();
    let (forum, batch) = asking(&mut state, OLD);
    apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({"id": OLD.to_string(), "channel_id": OLD.to_string(), "embeds": []}),
    );

    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(OLD, "first", None)),
        &mut Vec::new(),
    );
    let (forum, batch) = state.next_post_batch().expect("asked again");
    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(OLD, "first", None)),
        &mut Vec::new(),
    );

    assert_eq!(preview(&state, OLD).as_deref(), Some("first"));
    assert_eq!(state.next_post_batch(), None);
}

// An answer that says the first message is gone needs no second ask; an edit of a post nobody
// asked about is nothing to keep.
#[test]
fn a_deleted_answer_and_an_unasked_edit_change_nothing() {
    let mut state = forum_with_posts();
    let (forum, batch) = asking(&mut state, NEWEST);
    apply(
        &mut state,
        "MESSAGE_UPDATE",
        edit_at(NEWEST, "gone anyway", NOON),
    );
    let gone: crate::model::PostData = serde_json::from_str(
        &json!({"threads": {NEWEST.to_string(): {"first_message": null, "owner": null}}})
            .to_string(),
    )
    .unwrap();
    state.finish_post_batch(forum, &batch, Some(gone), &mut Vec::new());
    assert_eq!(preview(&state, NEWEST).as_deref(), Some("deleted"));
    assert_eq!(state.next_post_batch(), None);

    apply(
        &mut state,
        "MESSAGE_UPDATE",
        edit_at(MIDDLE, "unasked", LATER_THAT_DAY),
    );
    let (forum, batch) = asking(&mut state, MIDDLE);
    state.finish_post_batch(
        forum,
        &batch,
        Some(answered(MIDDLE, "as asked", None)),
        &mut Vec::new(),
    );
    assert_eq!(preview(&state, MIDDLE).as_deref(), Some("as asked"));
}

// H-4: a post read beside its forum never moves the forum's ack back to its newest post; every
// ack of the forum is at "now".
#[test]
fn a_post_read_beside_its_forum_leaves_the_forums_ack_at_now() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    load_page(&mut state, OLD, LoadKind::Latest, &[OLD + 1], true);
    let before = snowflake_at(wall_millis());
    let both = [Snowflake::new(FORUM), Snowflake::new(OLD)];

    let acks = state.set_reading(&both, &mut Vec::new());

    let forum: Vec<_> = acks.iter().filter(|ack| ack.channel == both[0]).collect();
    assert_eq!(forum.len(), 1, "{acks:?}");
    assert!(forum[0].now);
    assert!(acks.iter().any(|ack| ack.channel == both[1]), "{acks:?}");
    assert!(!state.unread(both[1]));
    let read = forum_ack(&state);
    assert!(read >= Some(before), "{read:?}");

    let later = snowflake_at(wall_millis() + 60_000);
    apply(&mut state, "MESSAGE_CREATE", message(later, OLD));

    let acks = state.take_reading_acks();
    assert!(
        acks.iter().all(|ack| ack.channel != both[0] || ack.now),
        "{acks:?}"
    );
    assert!(forum_ack(&state) >= read, "never back");
    state.set_reading(&[], &mut Vec::new());
    assert!(
        state.set_reading(&both, &mut Vec::new()).is_empty(),
        "both are read"
    );
}

fn own_post(id: u64) -> Value {
    let mut post = listed_post(id, id, &[]);
    post["owner_id"] = ME.to_string().into();
    post["newly_created"] = true.into();
    post
}

// H-4: the user's own post acks its forum up to it without a request (module 218461), counts as
// opened and is never new.
#[test]
fn the_users_own_post_acks_its_forum_locally() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    assert_eq!(posts_count(&state), PostCount::New(1));

    let events = apply(&mut state, "THREAD_CREATE", own_post(LATER));

    assert_eq!(forum_ack(&state), Some(LATER));
    assert!(state.take_reading_acks().is_empty(), "no request");
    assert_eq!(posts_count(&state), PostCount::None);
    assert!(
        events.contains(&format!("BadgeChanged({FORUM})")),
        "{events:?}"
    );
    let post = Snowflake::new(LATER);
    assert!(state.new_posts(&[post]).is_empty());
    assert!(
        state.read_state(post).is_some_and(|read| !read.estimated),
        "opened"
    );
    assert!(!state.unread(post));
    assert!(active(&state, &ForumView::default()).contains(&LATER));
}

#[test]
fn an_own_post_never_moves_the_forums_ack_back() {
    let mut state = forum_with_new_posts();
    apply(&mut state, "MESSAGE_ACK", read_ack(FORUM, LATER + 5));

    apply(&mut state, "THREAD_CREATE", own_post(LATER));

    assert_eq!(forum_ack(&state), Some(LATER + 5));
}

// A post of the user's that only comes into view (no `newly_created`) says nothing was read.
#[test]
fn an_own_post_that_isnt_new_leaves_the_forums_ack() {
    let mut state = forum_with_new_posts();
    let mut post = own_post(LATER);
    post["newly_created"] = false.into();

    apply(&mut state, "THREAD_CREATE", post);

    assert_eq!(forum_ack(&state), Some(FORUM_ACK));
}

// While the forum's page is read its ack goes to Discord as every other, at "now".
#[test]
fn an_own_post_in_a_forum_being_read_is_acked_at_now() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    read_forum(&mut state);
    let later = snowflake_at(wall_millis() + 60_000);

    apply(&mut state, "THREAD_CREATE", own_post(later));

    let acks = state.take_reading_acks();
    assert_eq!(acks.len(), 1, "{acks:?}");
    assert!(acks[0].now);
    assert!(forum_ack(&state) >= Some(later));
    assert!(state.new_posts(&[Snowflake::new(later)]).is_empty());
}

// H-5: how many messages a post has past the user's ack of it. Asked with op 28 for the open
// forum's active posts that are unread, opened and acked, each once a connection; kept current
// by messages and acks afterwards.

// OLD is unread past a real ack; MIDDLE is read; NEWEST was never opened.
fn forum_with_an_opened_unread_post() -> State {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    for (id, messages) in [(OLD, 9), (MIDDLE, 4), (NEWEST, 2)] {
        let mut post = started_when_made(listed_post(id, id + 5, &[]));
        post["message_count"] = messages.into();
        apply(&mut state, "THREAD_CREATE", post);
    }
    apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 2));
    apply(&mut state, "MESSAGE_ACK", read_ack(MIDDLE, MIDDLE + 5));
    open(&mut state);
    state
}

fn asked(state: &mut State) -> Vec<(u64, u64)> {
    let mut asked = state.next_forum_unreads().map_or_else(Vec::new, |ask| {
        assert_eq!(ask.guild.get(), G1);
        assert_eq!(ask.forum.get(), FORUM);
        ask.threads
            .iter()
            .map(|(post, ack)| (post.get(), ack.get()))
            .collect()
    });
    asked.sort_unstable();
    asked
}

fn unreads(counts: &[(u64, Option<u32>)]) -> Value {
    let threads: Vec<Value> = counts
        .iter()
        .map(|(post, count)| json!({"thread_id": post.to_string(), "count": count}))
        .collect();
    json!({"channel_id": FORUM.to_string(), "threads": threads})
}

fn new_counts(state: &State) -> Vec<(u64, Option<u32>)> {
    let posts = [OLD, MIDDLE, NEWEST].map(Snowflake::new);
    state
        .post_new_counts(&posts)
        .iter()
        .map(|count| (count.post.get(), count.count))
        .collect()
}

#[test]
fn unread_opened_posts_are_asked_once_a_connection() {
    let mut state = forum_with_an_opened_unread_post();

    assert_eq!(asked(&mut state), [(OLD, OLD + 2)]);
    assert_eq!(asked(&mut state), [], "asked already");

    state.forget_forum_unreads(&[(Snowflake::new(OLD), Snowflake::new(OLD + 2))]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "the frame never left");

    apply(&mut state, "READY", ready_value());
    apply(&mut state, "CHANNEL_CREATE", forum());
    let mut post = started_when_made(listed_post(OLD, OLD + 5, &[]));
    post["message_count"] = 9.into();
    apply(&mut state, "THREAD_CREATE", post);
    apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 2));
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "a new connection");
}

#[test]
fn no_forum_open_or_an_archived_post_asks_nothing() {
    let mut state = forum_with_an_opened_unread_post();
    let mut archived = started_when_made(listed_post(OLD, OLD + 5, &[]));
    archived["thread_metadata"]["archived"] = true.into();
    apply(&mut state, "THREAD_UPDATE", archived);
    assert_eq!(asked(&mut state), []);
    assert_eq!(new_counts(&state), []);

    let mut state = forum_with_an_opened_unread_post();
    state.open_forum(None, &mut Vec::new());
    assert_eq!(asked(&mut state), []);
}

#[test]
fn at_most_180_posts_are_asked_at_once() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum());
    for index in 0..200 {
        let id = 300_000_000_000_001_000 + index * 10;
        apply(
            &mut state,
            "THREAD_CREATE",
            started_when_made(listed_post(id, id + 5, &[])),
        );
        apply(&mut state, "MESSAGE_ACK", read_ack(id, id + 2));
    }
    open(&mut state);

    let first = asked(&mut state);
    let rest = asked(&mut state);

    assert_eq!(first.len(), 180);
    assert_eq!(rest.len(), 20);
    assert!(rest.iter().all(|post| !first.contains(post)));
}

#[test]
fn an_answer_shows_as_the_posts_new_count() {
    let mut state = forum_with_an_opened_unread_post();
    assert_eq!(new_counts(&state), [(OLD, None)], "unread, opened, acked");
    asked(&mut state);

    let events = apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    assert_eq!(events, [format!("PostCountsChanged({FORUM})")]);
    assert_eq!(new_counts(&state), [(OLD, Some(3))]);
}

#[test]
fn an_answer_is_read_leniently() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);

    apply(
        &mut state,
        "FORUM_UNREADS",
        json!({"channel_id": FORUM.to_string(), "threads": [
            {"thread_id": OLD.to_string(), "missing": true, "count": null},
            {"thread_id": "no id"},
        ]}),
    );
    assert_eq!(new_counts(&state), [(OLD, None)], "no count");
    assert_eq!(asked(&mut state), [], "still asked");

    let mut denied = unreads(&[(OLD, Some(3))]);
    denied["permission_denied"] = true.into();
    assert!(apply(&mut state, "FORUM_UNREADS", denied).is_empty());
    assert_eq!(new_counts(&state), [(OLD, None)]);

    apply(
        &mut state,
        "FORUM_UNREADS",
        json!({"channel_id": FORUM.to_string()}),
    );
    assert_eq!(new_counts(&state), [(OLD, None)]);
}

#[test]
fn a_count_is_never_above_the_posts_messages() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(40))]));
    assert_eq!(new_counts(&state), [(OLD, Some(9))]);

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(0))]));
    assert_eq!(new_counts(&state), [(OLD, None)], "1+");
}

#[test]
fn a_message_adds_one_to_a_known_count() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    let events = apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));

    assert!(
        events.contains(&format!("PostCountsChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(new_counts(&state), [(OLD, Some(4))]);
}

#[test]
fn a_message_in_a_read_post_without_a_count_says_one_or_more() {
    let mut state = forum_with_an_opened_unread_post();

    let events = apply(&mut state, "MESSAGE_CREATE", message(MIDDLE + 100, MIDDLE));

    assert!(
        events.contains(&format!("PostCountsChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(new_counts(&state), [(OLD, None), (MIDDLE, None)]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2), (MIDDLE, MIDDLE + 5)]);
}

// Akari's departure: any ack of the post restarts its count, where Discord's only follows
// another session's.
#[test]
fn reading_a_post_starts_its_count_at_zero() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    let events = apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 5));
    assert!(
        events.contains(&format!("PostCountsChanged({FORUM})")),
        "{events:?}"
    );
    assert_eq!(new_counts(&state), []);

    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));
    apply(&mut state, "MESSAGE_CREATE", message(OLD + 101, OLD));
    assert_eq!(new_counts(&state), [(OLD, Some(2))]);
    assert_eq!(asked(&mut state), [], "counted here");
}

#[test]
fn a_local_ack_starts_the_count_at_zero_too() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    let mut events = Vec::new();
    state.mark_read(Snowflake::new(OLD), &mut events);

    assert!(
        events
            .iter()
            .map(describe)
            .any(|event| event == format!("PostCountsChanged({FORUM})")),
        "{events:?}"
    );
    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));
    assert_eq!(new_counts(&state), [(OLD, Some(1))]);
}

#[test]
fn an_ack_that_leaves_messages_unread_forgets_the_count() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 3));

    assert_eq!(new_counts(&state), [(OLD, None)]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 3)], "asked again");
}

// An answer for an ack the post has moved on from says nothing about it now.
#[test]
fn an_answer_for_an_older_ack_is_dropped() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 5));

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));
    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));

    assert_eq!(new_counts(&state), [(OLD, Some(1))]);
}

#[test]
fn the_users_own_message_reads_the_post_and_counts_nothing() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));
    let mut own = message(OLD + 100, OLD);
    own["author"]["id"] = ME.to_string().into();

    apply(&mut state, "MESSAGE_CREATE", own);
    assert_eq!(new_counts(&state), []);

    apply(&mut state, "MESSAGE_CREATE", message(OLD + 101, OLD));
    assert_eq!(new_counts(&state), [(OLD, Some(1))]);
}

// The post's read state is there before the post, so no ack of the post sets the 0.
#[test]
fn a_new_post_of_the_open_forum_starts_at_zero() {
    let mut state = forum_with_an_opened_unread_post();
    apply(&mut state, "MESSAGE_ACK", read_ack(LATER, LATER));
    let mut post = started_when_made(listed_post(LATER, LATER, &[]));
    post["newly_created"] = true.into();
    apply(&mut state, "THREAD_CREATE", post);

    apply(&mut state, "MESSAGE_CREATE", message(LATER + 1, LATER));

    let count = state.post_new_counts(&[Snowflake::new(LATER)]);
    assert_eq!(count.len(), 1);
    assert_eq!(count[0].count, Some(1));
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "never the new post");
}

// A resume replays what the connection missed, and sometimes what it didn't.
#[test]
fn a_replayed_message_counts_once() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));
    apply(&mut state, "RESUMED", json!({}));
    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));

    assert_eq!(new_counts(&state), [(OLD, Some(4))]);
}

// The read state of the post made again is there before it, so no ack touches its count.
fn make_old_again(state: &mut State) {
    apply(state, "MESSAGE_ACK", read_ack(OLD, OLD + 2));
    let mut post = started_when_made(listed_post(OLD, OLD + 5, &[]));
    post["message_count"] = 9.into();
    apply(state, "THREAD_CREATE", post);
    state.refresh_forum(Snowflake::new(FORUM), &mut Vec::new());
}

#[test]
fn a_deleted_post_forgets_its_count() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": OLD.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );
    make_old_again(&mut state);

    assert_eq!(new_counts(&state), [(OLD, None)]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "asked again");
}

#[test]
fn a_deleted_post_that_was_only_asked_can_be_asked_again() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);

    apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": OLD.to_string(), "guild_id": G1.to_string(), "parent_id": FORUM.to_string(), "type": 11}),
    );
    make_old_again(&mut state);

    assert_eq!(asked(&mut state), [(OLD, OLD + 2)]);
}

// The forum's deletion takes its posts along without a THREAD_DELETE of their own.
#[test]
fn a_deleted_forums_posts_forget_their_counts() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": FORUM.to_string(), "guild_id": G1.to_string(), "type": 15}),
    );
    apply(&mut state, "CHANNEL_CREATE", forum());
    open(&mut state);
    make_old_again(&mut state);

    assert_eq!(new_counts(&state), [(OLD, None)]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "asked again");
}

// A refused ack is undone: the post is unread again and its count unknown, not a stale 0.
#[test]
fn an_undone_ack_forgets_the_count_it_started() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));
    let ack = state
        .mark_read(Snowflake::new(OLD), &mut Vec::new())
        .expect("an ack");

    let mut events = Vec::new();
    assert!(state.undo_ack(ack.channel, ack.before, ack.after, &mut events));

    assert!(
        events
            .iter()
            .map(describe)
            .any(|event| event == format!("PostCountsChanged({FORUM})")),
        "{events:?}"
    );
    apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));
    assert_eq!(new_counts(&state), [(OLD, None)]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)], "asked again");
}

// Asked with ack A, read partway to B, asked again with B: the late answer for A says nothing
// about B, and B's own answer is the count.
#[test]
fn a_late_answer_for_an_older_ask_is_dropped() {
    let mut state = forum_with_an_opened_unread_post();
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)]);
    apply(&mut state, "MESSAGE_ACK", read_ack(OLD, OLD + 3));
    assert_eq!(asked(&mut state), [(OLD, OLD + 3)]);

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));
    assert_eq!(
        new_counts(&state),
        [(OLD, None)],
        "the answer for the older ack"
    );

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(2))]));
    assert_eq!(new_counts(&state), [(OLD, Some(2))]);
}

// A frame that never left has no answer on its way.
#[test]
fn a_frame_that_never_left_leaves_no_answer_to_wait_for() {
    let mut state = forum_with_an_opened_unread_post();
    asked(&mut state);
    state.forget_forum_unreads(&[(Snowflake::new(OLD), Snowflake::new(OLD + 2))]);
    assert_eq!(asked(&mut state), [(OLD, OLD + 2)]);

    apply(&mut state, "FORUM_UNREADS", unreads(&[(OLD, Some(3))]));

    assert_eq!(new_counts(&state), [(OLD, Some(3))]);
}

// Only a count that changed, or a post that may now show one, is worth a read.
#[test]
fn a_message_in_a_post_that_shows_no_count_says_nothing() {
    let mut state = forum_with_an_opened_unread_post();
    let changed = format!("PostCountsChanged({FORUM})");

    let never_opened = apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 100, NEWEST));
    assert!(!never_opened.contains(&changed), "{never_opened:?}");

    let already_unread = apply(&mut state, "MESSAGE_CREATE", message(OLD + 100, OLD));
    assert!(!already_unread.contains(&changed), "{already_unread:?}");
}

// H-7: who may post in a forum, and with which tags.

const VIEW: u64 = 1 << 10;
const SEND: u64 = 1 << 11;
const MANAGE_THREADS: u64 = 1 << 34;

// The fixture's user owns the guild; here they're a plain member whose one role allows `allow`.
fn member_with_forum(allow: u64, edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    let guild = &mut ready["guilds"][0];
    guild["properties"]["owner_id"] = "100000000000000099".into();
    guild["roles"][0]["permissions"] = "0".into();
    guild["roles"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "600", "name": "r", "permissions": allow.to_string(), "position": 2}));
    ready["merged_members"][0][0]["roles"] = json!(["600"]);
    edit(&mut ready);
    let mut state = state_from(&ready);
    apply(&mut state, "CHANNEL_CREATE", open_forum());
    state
}

// The fixture's channel denies @everyone sending.
fn open_forum() -> Value {
    let mut forum = forum();
    forum["permission_overwrites"] = json!([]);
    forum
}

fn creation(state: &State, channel: u64) -> (bool, Vec<u64>, bool) {
    let creation = state.post_creation(Snowflake::new(channel), state.now_millis());
    (
        creation.allowed,
        creation.tags.iter().map(|tag| tag.get()).collect(),
        creation.requires_tag,
    )
}

#[test]
fn posting_needs_send_messages_in_the_forum() {
    let member = member_with_forum(VIEW | SEND, |_| {});
    assert_eq!(creation(&member, FORUM), (true, vec![BUG], false));

    let reader = member_with_forum(VIEW, |_| {});
    assert_eq!(creation(&reader, FORUM), (false, vec![], false));

    let timed_out = member_with_forum(VIEW | SEND, |ready| {
        ready["merged_members"][0][0]["communication_disabled_until"] =
            "2999-01-01T00:00:00+00:00".into();
    });
    assert_eq!(creation(&timed_out, FORUM), (false, vec![], false));
}

#[test]
fn moderated_tags_need_manage_threads() {
    let moderator = member_with_forum(VIEW | SEND | MANAGE_THREADS, |_| {});
    assert_eq!(creation(&moderator, FORUM), (true, vec![BUG, STAFF], false));

    let mut owner = ready_state();
    apply(&mut owner, "CHANNEL_CREATE", forum());
    assert_eq!(creation(&owner, FORUM), (true, vec![BUG, STAFF], false));
}

#[test]
fn a_forum_can_require_a_tag() {
    let mut state = ready_state();
    let mut forum = forum();
    forum["flags"] = (1 << 4).into();
    apply(&mut state, "CHANNEL_CREATE", forum);

    assert_eq!(creation(&state, FORUM), (true, vec![BUG, STAFF], true));
}

#[test]
fn only_forum_and_media_channels_take_posts() {
    let mut state = ready_state();
    let mut media = forum();
    media["type"] = 16.into();
    apply(&mut state, "CHANNEL_CREATE", media);

    assert!(creation(&state, FORUM).0, "a media channel");
    assert_eq!(creation(&state, GENERAL), (false, vec![], false));
    assert_eq!(creation(&state, 1), (false, vec![], false), "unknown");
}

// H-9: a forum's slowmode for new posts is its `rate_limit_per_user`.
#[test]
fn a_forums_slowmode_is_its_cooldown_between_posts() {
    let mut state = member_with_forum(VIEW | SEND, |_| {});
    let mut slow = open_forum();
    slow["rate_limit_per_user"] = 45.into();
    apply(&mut state, "CHANNEL_UPDATE", slow);
    let forum = Snowflake::new(FORUM);
    let read = |state: &State| {
        state.thread_slowmode(
            forum,
            state.now_millis(),
            std::time::Instant::now(),
            std::time::SystemTime::now(),
        )
    };

    let idle = read(&state).expect("slowmode");
    assert_eq!(idle.interval, std::time::Duration::from_secs(45));
    assert!(!idle.exempt);
    assert_eq!(idle.until, None);

    state.start_thread_cooldown(forum, state.now_millis());
    assert!(read(&state).is_some_and(|slowmode| slowmode.until.is_some()));
}

// Discord's client waits 100 ms past the `retry_after` of a 20016 too.
#[test]
fn discords_wait_is_held_with_the_clients_margin() {
    use std::time::{Duration, Instant, SystemTime};
    let mut state = member_with_forum(VIEW | SEND, |_| {});
    let mut slow = open_forum();
    slow["rate_limit_per_user"] = 45.into();
    apply(&mut state, "CHANNEL_UPDATE", slow);
    let forum = Snowflake::new(FORUM);
    let (now, wall) = (Instant::now(), SystemTime::now());

    state.hold_thread_cooldown(forum, Duration::from_secs(5), state.now_millis());

    let until = state
        .thread_slowmode(forum, state.now_millis(), now, wall)
        .and_then(|slowmode| slowmode.until)
        .expect("held");
    assert!(until >= wall + Duration::from_millis(5100), "{until:?}");
    assert!(until < wall + Duration::from_secs(6), "{until:?}");
}

fn created_post(id: u64) -> crate::model::Channel {
    let mut post = own_post(id);
    post["member"] = json!({"id": id.to_string(), "user_id": ME.to_string(), "join_timestamp": "2026-10-01T00:00:00+00:00", "flags": 1});
    post.as_object_mut().unwrap().remove("newly_created");
    serde_json::from_str(&post.to_string()).unwrap()
}

fn first_message(post: u64) -> crate::model::Message {
    let mut first = message(post, post);
    first["author"]["id"] = ME.to_string().into();
    first["content"] = "steps to reproduce".into();
    serde_json::from_str(&first.to_string()).unwrap()
}

// H-8: the answer to a create is the post as its THREAD_CREATE would bring it, whichever comes
// first, and its message is the post's first.
#[test]
fn a_created_post_is_kept_as_the_users_own_new_post() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    let mut events = Vec::new();

    let (kept, acks) =
        state.keep_created_post(created_post(LATER), Some(first_message(LATER)), &mut events);

    let post = Snowflake::new(LATER);
    assert_eq!(kept, Some(post));
    assert!(acks.is_empty(), "no request");
    assert!(active(&state, &ForumView::default()).contains(&LATER));
    assert_eq!(forum_ack(&state), Some(LATER));
    assert!(state.read_state(post).is_some_and(|read| !read.estimated));
    assert!(state.new_posts(&[post]).is_empty());
    assert_eq!(
        preview(&state, LATER).as_deref(),
        Some("steps to reproduce")
    );
    assert!(
        state
            .channel(post)
            .is_some_and(|post| { post.thread.as_deref().is_some_and(|info| info.joined()) })
    );

    apply(&mut state, "THREAD_CREATE", own_post(LATER));
    let posts = state.forum_posts(Snowflake::new(FORUM), &ForumView::default());
    assert_eq!(posts.new_posts, 0);
    assert_eq!(
        posts
            .active
            .iter()
            .filter(|post| post.id.get() == LATER)
            .count(),
        1
    );
    assert_eq!(forum_ack(&state), Some(LATER));
}

#[test]
fn an_answer_after_the_gateways_post_changes_nothing() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    apply(&mut state, "THREAD_CREATE", own_post(LATER));
    let before = state.forum_posts(Snowflake::new(FORUM), &ForumView::default());

    let (kept, _) = state.keep_created_post(created_post(LATER), None, &mut Vec::new());

    assert_eq!(kept, Some(Snowflake::new(LATER)));
    let after = state.forum_posts(Snowflake::new(FORUM), &ForumView::default());
    assert_eq!(after.new_posts, 0);
    assert_eq!(after.active.len(), before.active.len());
    assert_eq!(forum_ack(&state), Some(LATER));
}

#[test]
fn a_created_post_in_a_forum_being_read_is_acked_at_now() {
    let mut state = forum_with_new_posts();
    open(&mut state);
    read_forum(&mut state);
    let later = snowflake_at(wall_millis() + 60_000);

    let (_, acks) = state.keep_created_post(created_post(later), None, &mut Vec::new());

    assert_eq!(acks.len(), 1, "{acks:?}");
    assert!(acks[0].now);
    assert!(forum_ack(&state) >= Some(later));
}
