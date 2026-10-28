use super::*;
use crate::model::{ChannelId, MessageId};
use crate::model::{EmojiId, TagId};
use crate::state::apply::wall_millis;
use crate::state::{
    ForumLayout, ForumSort, ForumTag, ForumView, PostCount, PostPreview, StoreEvent, TagEmoji,
    TagMatching,
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
    apply(&mut state, "THREAD_CREATE", listed_post(OLD, OLD + 1, &[]));
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
