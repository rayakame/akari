use super::*;
use crate::state::{NotificationLevel, ThreadMembership};

// From thread_create.json: created in GENERAL by Mira, with the user's member.
const CREATED: u64 = 300_000_000_000_000_021;
// The fixture's member joined G1 on 2024-01-01; these IDs are from 2023 and 2024-10.
const BEFORE_JOIN: u64 = 1_100_000_000_000_000_000;
const AFTER_JOIN: u64 = 1_300_000_000_000_000_000;

fn ready_where(edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    edit(&mut ready);
    state_from(&ready)
}

fn membership(state: &State, thread: u64) -> Option<ThreadMembership> {
    state
        .channel(Snowflake::new(thread))
        .and_then(|channel| channel.thread.as_deref().map(|info| info.member))
        .expect("a known thread")
}

fn joined(state: &State, thread: u64) -> bool {
    membership(state, thread).is_some()
}

fn created() -> Value {
    fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ))
}

fn created_unjoined() -> Value {
    let mut thread = created();
    thread.as_object_mut().unwrap().remove("member");
    thread
}

fn member(user: u64, flags: u64) -> Value {
    json!({
        "id": CREATED.to_string(), "user_id": user.to_string(),
        "join_timestamp": "2026-10-08T12:05:00.000000+00:00",
        "flags": flags, "muted": false, "mute_config": null
    })
}

fn members_update(thread: u64, added: Vec<Value>, removed: &[u64]) -> Value {
    json!({
        "id": thread.to_string(), "guild_id": G1.to_string(), "member_count": 7,
        "added_members": added,
        "removed_member_ids": removed.iter().map(u64::to_string).collect::<Vec<_>>()
    })
}

fn mute_ending(end: &str) -> Value {
    json!({"selected_time_window": 900, "end_time": end})
}

fn at(unix_millis: i64) -> Option<Timestamp> {
    Some(Timestamp::from_unix_millis(unix_millis))
}

// A message from Mira that mentions nobody.
fn plain(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["mentions"] = json!([]);
    message
}

#[test]
fn a_ready_thread_is_joined_with_its_members_mute() {
    let state = ready_where(|ready| {
        let member = &mut ready["guilds"][0]["threads"][0]["member"];
        member["muted"] = true.into();
        member["mute_config"] = mute_ending("2026-10-20T00:00:00.000000+00:00");
        member["flags"] = (1 | 4).into();
    });

    assert_eq!(
        membership(&state, THREAD),
        Some(ThreadMembership {
            muted: true,
            mute_ends: at(1_792_454_400_000),
            level: Some(NotificationLevel::OnlyMentions),
        })
    );
}

#[test]
fn a_ready_thread_without_a_member_is_joined() {
    let state = ready_where(|ready| {
        ready["guilds"][0]["threads"][0]
            .as_object_mut()
            .unwrap()
            .remove("member");
    });

    assert_eq!(
        membership(&state, THREAD),
        Some(ThreadMembership::default())
    );
}

#[test]
fn a_thread_create_with_our_member_joins() {
    let mut state = ready_state();

    apply(&mut state, "THREAD_CREATE", created());

    assert_eq!(
        membership(&state, CREATED),
        Some(ThreadMembership::default())
    );
}

#[test]
fn a_thread_create_without_a_member_keeps_the_membership() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created());

    let mut renamed = created_unjoined();
    renamed["name"] = "Renamed".into();
    apply(&mut state, "THREAD_CREATE", renamed);

    assert!(joined(&state, CREATED));
}

#[test]
fn a_members_update_with_our_id_joins_the_thread() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created_unjoined());
    assert!(!joined(&state, CREATED));

    let events = apply(
        &mut state,
        "THREAD_MEMBERS_UPDATE",
        members_update(CREATED, vec![member(ME, 2)], &[]),
    );

    assert_eq!(
        membership(&state, CREATED),
        Some(ThreadMembership {
            level: Some(NotificationLevel::AllMessages),
            ..ThreadMembership::default()
        })
    );
    assert!(
        events.contains(&format!("ChannelUpdated({CREATED})")),
        "{events:?}"
    );
}

#[test]
fn our_removal_unjoins_it() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created());

    let events = apply(
        &mut state,
        "THREAD_MEMBERS_UPDATE",
        members_update(CREATED, Vec::new(), &[ME]),
    );

    assert!(!joined(&state, CREATED));
    assert!(
        events.contains(&format!("ChannelUpdated({CREATED})")),
        "{events:?}"
    );
}

#[test]
fn others_in_a_members_update_change_nothing() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created());

    let events = apply(
        &mut state,
        "THREAD_MEMBERS_UPDATE",
        members_update(CREATED, vec![member(MIRA, 2)], &[100_000_000_000_000_003]),
    );

    assert!(events.is_empty(), "{events:?}");
    assert!(joined(&state, CREATED));
    let info = state
        .channel(Snowflake::new(CREATED))
        .unwrap()
        .thread
        .clone();
    assert_eq!(info.and_then(|info| info.member_count), Some(1));
}

#[test]
fn a_member_update_sets_our_mute_and_level() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created_unjoined());
    let mut update = member(ME, 1 | 4);
    update["guild_id"] = G1.to_string().into();
    update["muted"] = true.into();
    update["mute_config"] = mute_ending("2026-10-08T12:15:00.000000+00:00");

    let events = apply(&mut state, "THREAD_MEMBER_UPDATE", update);

    assert_eq!(
        membership(&state, CREATED),
        Some(ThreadMembership {
            muted: true,
            mute_ends: at(1_791_461_700_000),
            level: Some(NotificationLevel::OnlyMentions),
        })
    );
    assert!(
        events.contains(&format!("ChannelUpdated({CREATED})")),
        "{events:?}"
    );
}

#[test]
fn a_member_update_for_someone_else_is_skipped() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created_unjoined());
    let mut update = member(MIRA, 4);
    update["guild_id"] = G1.to_string().into();

    let events = apply(&mut state, "THREAD_MEMBER_UPDATE", update);

    assert!(events.is_empty(), "{events:?}");
    assert!(!joined(&state, CREATED));
}

#[test]
fn membership_events_for_an_unknown_thread_are_skipped() {
    let mut state = ready_state();
    let mut update = member(ME, 2);
    update["id"] = "300000000000000099".into();
    update["guild_id"] = G1.to_string().into();

    let mut events = apply(&mut state, "THREAD_MEMBER_UPDATE", update);
    events.extend(apply(
        &mut state,
        "THREAD_MEMBERS_UPDATE",
        members_update(300_000_000_000_000_099, vec![member(ME, 2)], &[]),
    ));

    assert!(events.is_empty(), "{events:?}");
    assert!(
        state
            .channel(Snowflake::new(300_000_000_000_000_099))
            .is_none()
    );
}

#[test]
fn a_thread_without_a_read_state_is_unread_from_its_start() {
    let mut state = ready_state();
    let mut thread = created();
    thread["last_message_id"] = BEFORE_JOIN.to_string().into();

    apply(&mut state, "THREAD_CREATE", thread);

    assert!(state.unread(Snowflake::new(CREATED)));
    assert!(state.read_state(Snowflake::new(CREATED)).is_none());
}

// A forum post's starter message has the post's own ID.
#[test]
fn a_forum_post_with_only_its_starter_is_unread() {
    let mut state = ready_state();
    let mut post = created();
    post["last_message_id"] = CREATED.to_string().into();

    apply(&mut state, "THREAD_CREATE", post);

    assert!(state.unread(Snowflake::new(CREATED)));
}

#[test]
fn a_thread_we_are_mentioned_into_is_unread_from_its_start() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", created_unjoined());
    apply(&mut state, "MESSAGE_CREATE", plain(AFTER_JOIN + 1, CREATED));

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(AFTER_JOIN + 2, CREATED),
    );

    let read = state.read_state(Snowflake::new(CREATED)).expect("started");
    assert_eq!(read.acked.map(|id| id.get()), Some(CREATED - 1));
    assert_eq!(read.mentions, 1);
    assert!(state.unread(Snowflake::new(CREATED)));
}

const MINUTE: i64 = 60_000;

fn snowflake_at(unix_millis: i64) -> u64 {
    Snowflake::<model::GenericMarker>::from_unix_millis(unix_millis, 0).get()
}

fn listed(state: &State, thread: u64) -> bool {
    state.listed(Snowflake::new(thread))
}

// READY's thread with its newest message `minutes_ago`, read up to it, hidden after an hour.
fn ready_thread(minutes_ago: i64, edit: impl FnOnce(&mut Value)) -> State {
    let last = snowflake_at(wall_millis() - minutes_ago * MINUTE);
    ready_where(|ready| {
        let thread = &mut ready["guilds"][0]["threads"][0];
        thread["last_message_id"] = last.to_string().into();
        thread["thread_metadata"]["auto_archive_duration"] = 60.into();
        ready["read_state"]["entries"][1]["last_message_id"] = last.to_string().into();
        edit(ready);
    })
}

fn listing_events(events: &[String]) -> Vec<&String> {
    events
        .iter()
        .filter(|event| event.starts_with("ThreadListingChanged"))
        .collect()
}

#[test]
fn a_thread_inside_its_window_is_listed() {
    let state = ready_thread(30, |_| {});

    assert!(listed(&state, THREAD));
}

#[test]
fn a_read_thread_past_its_window_isnt() {
    let state = ready_thread(61, |_| {});

    assert!(!listed(&state, THREAD));
}

#[test]
fn an_unread_thread_stays_listed_past_its_window() {
    let state = ready_thread(120, |ready| {
        let older = snowflake_at(wall_millis() - 130 * MINUTE);
        ready["read_state"]["entries"][1]["last_message_id"] = older.to_string().into();
    });

    assert!(listed(&state, THREAD));
}

#[test]
fn a_mentioned_thread_stays_listed_past_its_window() {
    let state = ready_thread(120, |ready| {
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
    });

    assert!(listed(&state, THREAD));
}

#[test]
fn reading_a_thread_past_its_window_unlists_it() {
    let last = snowflake_at(wall_millis() - 120 * MINUTE);
    let mut state = ready_thread(120, |ready| {
        let older = snowflake_at(wall_millis() - 130 * MINUTE);
        ready["read_state"]["entries"][1]["last_message_id"] = older.to_string().into();
    });

    let events = apply(
        &mut state,
        "MESSAGE_ACK",
        json!({"channel_id": THREAD.to_string(), "message_id": last.to_string(), "version": 2}),
    );

    assert!(!listed(&state, THREAD));
    assert_eq!(
        listing_events(&events),
        [&format!("ThreadListingChanged({THREAD})")]
    );
}

#[test]
fn a_message_past_the_window_lists_it_again() {
    let mut state = ready_thread(120, |_| {});
    assert!(!listed(&state, THREAD));

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(snowflake_at(wall_millis()), THREAD),
    );

    assert!(listed(&state, THREAD));
    assert_eq!(
        listing_events(&events),
        [&format!("ThreadListingChanged({THREAD})")]
    );
}

#[test]
fn an_own_message_past_the_window_lists_it_again() {
    let mut state = ready_thread(120, |_| {});
    let mut own = plain(snowflake_at(wall_millis()), THREAD);
    own["author"]["id"] = ME.to_string().into();

    let events = apply(&mut state, "MESSAGE_CREATE", own);

    assert!(!state.unread(Snowflake::new(THREAD)));
    assert!(listed(&state, THREAD));
    assert_eq!(listing_events(&events).len(), 1, "{events:?}");
}

#[test]
fn a_pinned_post_has_no_window() {
    let state = ready_thread(10_000, |ready| {
        ready["guilds"][0]["threads"][0]["flags"] = 2.into();
    });

    assert!(listed(&state, THREAD));
}

#[test]
fn the_window_counts_from_the_later_of_message_and_archive_timestamp() {
    let state = ready_thread(120, |ready| {
        ready["guilds"][0]["threads"][0]["thread_metadata"]["archive_timestamp"] =
            model::iso(wall_millis() - 30 * MINUTE).into();
    });

    assert!(listed(&state, THREAD));
}

fn muted_member() -> Option<ThreadMembership> {
    Some(ThreadMembership {
        muted: true,
        ..ThreadMembership::default()
    })
}

fn ready_with_a_muted_thread() -> State {
    ready_where(|ready| {
        ready["guilds"][0]["threads"][0]["member"]["muted"] = true.into();
    })
}

// READY's guild as GUILD_CREATE sends it, its thread without the user's member.
fn guild_create_without_members() -> Value {
    let mut guild = ready_value()["guilds"][0].clone();
    guild["threads"][0]
        .as_object_mut()
        .unwrap()
        .remove("member");
    guild
}

#[test]
fn a_guild_create_after_an_outage_keeps_a_threads_own_mute() {
    let mut state = ready_with_a_muted_thread();
    apply(
        &mut state,
        "GUILD_DELETE",
        json!({"id": G1.to_string(), "unavailable": true}),
    );

    apply(&mut state, "GUILD_CREATE", guild_create_without_members());

    assert_eq!(membership(&state, THREAD), muted_member());
}

#[test]
fn a_repeated_guild_create_keeps_a_threads_own_mute() {
    let mut state = ready_with_a_muted_thread();

    apply(&mut state, "GUILD_CREATE", guild_create_without_members());

    assert_eq!(membership(&state, THREAD), muted_member());
}

#[test]
fn a_ready_without_the_member_keeps_a_threads_own_mute() {
    let mut state = ready_with_a_muted_thread();
    let mut ready = ready_value();
    ready["guilds"][0]["threads"][0]
        .as_object_mut()
        .unwrap()
        .remove("member");

    apply(&mut state, "READY", ready);

    assert_eq!(membership(&state, THREAD), muted_member());
}

#[test]
fn a_guild_down_at_ready_keeps_its_threads_mutes_for_its_return() {
    let mut state = ready_with_a_muted_thread();
    let mut ready = ready_value();
    ready["guilds"][0] = json!({"id": G1.to_string(), "unavailable": true});
    apply(&mut state, "READY", ready);

    apply(&mut state, "GUILD_CREATE", guild_create_without_members());

    assert_eq!(membership(&state, THREAD), muted_member());
}
