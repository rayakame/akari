use super::*;

const ROLE: u64 = 500_000_000_000_000_002;
const JUN: u64 = 100_000_000_000_000_003;
const BLOCKED: u64 = 100_000_000_000_000_004;
const FORUM: u64 = 300_000_000_000_000_030;
const UNJOINED: u64 = 300_000_000_000_000_021;
// The fixture's member joined G1 on 2024-01-01; these IDs are from 2023 and 2024-10.
const BEFORE_JOIN: u64 = 1_100_000_000_000_000_000;
const AFTER_JOIN: u64 = 1_300_000_000_000_000_000;

fn ready_where(edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    edit(&mut ready);
    state_from(&ready)
}

fn unread(state: &State, channel: u64) -> bool {
    state.unread(Snowflake::new(channel))
}

fn read_state(state: &State, channel: u64) -> Option<(Option<u64>, u32)> {
    state
        .read_state(Snowflake::new(channel))
        .map(|read| (read.acked.map(|id| id.get()), read.mentions))
}

fn mentions(state: &State, channel: u64) -> u32 {
    read_state(state, channel).map_or(0, |(_, mentions)| mentions)
}

// A message from Mira that mentions nobody.
fn plain(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["mentions"] = json!([]);
    message
}

fn dm_plain(id: u64, channel: u64) -> Value {
    let mut message = dm_message(id, channel);
    message["mentions"] = json!([]);
    message
}

fn by(mut message: Value, author: u64) -> Value {
    message["author"]["id"] = author.to_string().into();
    message
}

fn ack(channel: u64, message: u64) -> Value {
    json!({"channel_id": channel.to_string(), "message_id": message.to_string(), "version": 1})
}

fn passive(guild: u64, channel: u64, last: u64) -> Value {
    json!({
        "guild_id": guild.to_string(),
        "updated_channels": [{"id": channel.to_string(), "last_message_id": last.to_string()}]
    })
}

fn general_read() -> State {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_003),
    );
    assert!(!unread(&state, GENERAL));
    state
}

fn without_read_state(ready: &mut Value, channel: u64) {
    let id = channel.to_string();
    let entries = ready["read_state"]["entries"].as_array_mut().unwrap();
    let before = entries.len();
    entries.retain(|entry| entry["id"].as_str() != Some(id.as_str()));
    assert_eq!(entries.len(), before - 1, "no read state for {channel}");
}

#[test]
fn a_channel_is_unread_when_its_newest_message_is_past_the_ack() {
    let state = ready_state();

    assert!(unread(&state, GENERAL), "acked 2, newest 3");
    assert!(!unread(&state, THREAD), "acked at its newest message");
    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );
}

#[test]
fn a_channel_acked_at_its_newest_message_is_read() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_003),
    );

    assert!(!unread(&state, GENERAL));
}

#[test]
fn a_guild_channel_without_a_read_state_is_unread_for_messages_after_the_join() {
    let state = ready_where(|ready| {
        without_read_state(ready, GENERAL);
        ready["guilds"][0]["channels"][1]["last_message_id"] = AFTER_JOIN.to_string().into();
    });

    assert!(unread(&state, GENERAL));
    assert_eq!(read_state(&state, GENERAL), None);
}

#[test]
fn a_guild_channel_without_a_read_state_is_read_for_messages_before_the_join() {
    let state = ready_where(|ready| {
        without_read_state(ready, GENERAL);
        ready["guilds"][0]["channels"][1]["last_message_id"] = BEFORE_JOIN.to_string().into();
    });

    assert!(!unread(&state, GENERAL));
}

#[test]
fn a_new_message_after_the_join_makes_a_channel_without_a_read_state_unread() {
    let mut state = ready_where(|ready| {
        without_read_state(ready, GENERAL);
        ready["guilds"][0]["channels"][1]["last_message_id"] = BEFORE_JOIN.to_string().into();
    });

    apply(&mut state, "MESSAGE_CREATE", plain(AFTER_JOIN, GENERAL));

    assert!(unread(&state, GENERAL));
}

#[test]
fn a_dm_without_a_read_state_is_read_until_a_message_arrives() {
    let mut state = ready_state();
    assert!(!unread(&state, GROUP), "no read state and a newest message");
    assert_eq!(read_state(&state, GROUP), None);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_plain(400_000_000_000_000_022, GROUP),
    );

    assert!(unread(&state, GROUP));
    assert_eq!(
        read_state(&state, GROUP),
        Some((Some(400_000_000_000_000_021), 1))
    );
}

#[test]
fn a_channel_without_messages_is_read() {
    let state = ready_where(|ready| {
        ready["read_state"]["entries"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": VOICE.to_string(), "last_message_id": null}));
    });

    assert!(!unread(&state, VOICE));
    assert!(!unread(&state, 300_000_000_000_000_001), "a category");
}

#[test]
fn a_read_state_without_a_position_counts_every_message() {
    let state = ready_where(|ready| {
        ready["read_state"]["entries"][0]["last_message_id"] = Value::Null;
    });

    assert!(unread(&state, GENERAL));
}

#[test]
fn a_deleted_newest_message_leaves_the_channel_unread() {
    let mut state = general_read();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "400000000000000004", "channel_id": GENERAL.to_string(), "guild_id": G1.to_string()}),
    );

    assert!(unread(&state, GENERAL));
    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_004));
}

#[test]
fn a_direct_mention_counts() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 3);
}

#[test]
fn a_plain_message_doesnt_count() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 2);
}

#[test]
fn everyone_counts_unless_suppressed() {
    let everyone = || {
        let mut message = plain(400_000_000_000_000_004, GENERAL);
        message["mention_everyone"] = true.into();
        message
    };
    let mut suppressed = ready_state();
    let mut allowed = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["suppress_everyone"] = false.into();
    });

    apply(&mut suppressed, "MESSAGE_CREATE", everyone());
    apply(&mut allowed, "MESSAGE_CREATE", everyone());

    assert_eq!(mentions(&suppressed, GENERAL), 2);
    assert_eq!(mentions(&allowed, GENERAL), 3);
}

#[test]
fn a_role_mention_counts_unless_suppressed() {
    let mentioning = |role: u64| {
        let mut message = plain(400_000_000_000_000_004, GENERAL);
        message["mention_roles"] = json!([role.to_string()]);
        message
    };
    let mut allowed = ready_state();
    let mut suppressed = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["suppress_roles"] = true.into();
    });
    let mut other_role = ready_state();

    apply(&mut allowed, "MESSAGE_CREATE", mentioning(ROLE));
    apply(&mut suppressed, "MESSAGE_CREATE", mentioning(ROLE));
    apply(
        &mut other_role,
        "MESSAGE_CREATE",
        mentioning(500_000_000_000_000_009),
    );

    assert_eq!(mentions(&allowed, GENERAL), 3);
    assert_eq!(mentions(&suppressed, GENERAL), 2);
    assert_eq!(mentions(&other_role, GENERAL), 2);
}

#[test]
fn every_dm_message_counts() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_plain(400_000_000_000_000_022, GROUP),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_plain(400_000_000_000_000_023, GROUP),
    );

    assert_eq!(mentions(&state, GROUP), 2);
}

#[test]
fn a_muted_dm_counts_only_mentions() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_plain(400_000_000_000_000_022, DM),
    );
    assert_eq!(mentions(&state, DM), 0);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_023, DM),
    );

    assert_eq!(mentions(&state, DM), 1);
}

#[test]
fn a_recipient_removed_message_doesnt_count() {
    let mut state = ready_state();
    let mut removed = dm_plain(400_000_000_000_000_022, GROUP);
    removed["type"] = 2.into();

    apply(&mut state, "MESSAGE_CREATE", removed);

    assert_eq!(mentions(&state, GROUP), 0);
}

#[test]
fn a_muted_channel_still_counts_mentions() {
    let mut state = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["channel_overrides"][1]["mute_config"] =
            Value::Null;
    });
    assert!(state.channel_muted_at(Snowflake::new(GENERAL), Timestamp::from_unix_millis(0)));

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 3);
}

#[test]
fn own_messages_reset_the_count_and_ack() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(plain(400_000_000_000_000_004, GENERAL), ME),
    );

    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_004), 0))
    );
    assert!(!unread(&state, GENERAL));
}

#[test]
fn an_own_message_in_a_dm_without_a_read_state_leaves_it_read() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(dm_plain(400_000_000_000_000_022, GROUP), ME),
    );

    assert!(!unread(&state, GROUP));
    assert_eq!(mentions(&state, GROUP), 0);
}

#[test]
fn a_poll_result_of_mine_doesnt_ack() {
    let mut state = ready_state();
    let mut result = by(plain(400_000_000_000_000_004, GENERAL), ME);
    result["type"] = 46.into();

    apply(&mut state, "MESSAGE_CREATE", result);

    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );
}

#[test]
fn a_blocked_authors_mention_doesnt_count() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(message(400_000_000_000_000_004, GENERAL), BLOCKED),
    );

    assert_eq!(mentions(&state, GENERAL), 2);
}

#[test]
fn an_ignored_authors_mention_doesnt_count() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(message(400_000_000_000_000_004, GENERAL), JUN),
    );

    assert_eq!(mentions(&state, GENERAL), 2);
}

#[test]
fn relationship_events_change_whose_mentions_count() {
    let mut state = ready_state();
    let relationship =
        |id: u64, kind: u8| json!({"id": id.to_string(), "type": kind, "user_ignored": false});

    apply(&mut state, "RELATIONSHIP_ADD", relationship(MIRA, 2));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_004, GENERAL),
    );
    assert_eq!(mentions(&state, GENERAL), 2, "Mira is blocked");

    apply(&mut state, "RELATIONSHIP_REMOVE", relationship(MIRA, 2));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_005, GENERAL),
    );
    assert_eq!(mentions(&state, GENERAL), 3, "Mira is unblocked");

    apply(
        &mut state,
        "RELATIONSHIP_UPDATE",
        json!({"id": JUN.to_string(), "type": 1, "user_ignored": false}),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(message(400_000_000_000_000_006, GENERAL), JUN),
    );
    assert_eq!(mentions(&state, GENERAL), 4, "Jun isn't ignored any more");

    apply(&mut state, "RELATIONSHIP_ADD", relationship(JUN, 1));
    apply(
        &mut state,
        "RELATIONSHIP_UPDATE",
        json!({"id": JUN.to_string(), "type": 1, "user_ignored": true}),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        by(message(400_000_000_000_000_007, GENERAL), JUN),
    );
    assert_eq!(mentions(&state, GENERAL), 4, "Jun is ignored again");
}

#[test]
fn a_silent_mention_counts() {
    let mut state = ready_state();
    let mut silent = message(400_000_000_000_000_004, GENERAL);
    silent["flags"] = (1 << 12).into();

    apply(&mut state, "MESSAGE_CREATE", silent);

    assert_eq!(mentions(&state, GENERAL), 3);
}

#[test]
fn a_message_in_a_channel_the_user_cant_view_counts_nothing() {
    let mut state = ready_where(|ready| {
        ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
        ready["merged_members"][0][0]["roles"] = json!([]);
        ready["guilds"][0]["channels"][1]["permission_overwrites"] =
            json!([{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]);
    });
    assert_eq!(
        state
            .permissions(Snowflake::new(GENERAL), 0)
            .map(|permissions| permissions.contains(Permissions::VIEW_CHANNEL)),
        Some(false)
    );

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 2);
}

#[test]
fn deleting_a_mentioning_message_keeps_the_count() {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_004, GENERAL),
    );

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "400000000000000004", "channel_id": GENERAL.to_string(), "guild_id": G1.to_string()}),
    );

    assert_eq!(mentions(&state, GENERAL), 3);
    assert!(unread(&state, GENERAL));
}

#[test]
fn mention_on_all_messages_counts_every_message_where_all_messages_notify() {
    let all_messages = |flags: u64| {
        ready_where(|ready| {
            ready["notification_settings"]["flags"] = flags.into();
            ready["user_guild_settings"]["entries"][0]["channel_overrides"][1]["message_notifications"] =
                0.into();
        })
    };
    let mut on = all_messages(16 | 32);
    let mut off = all_messages(16);

    apply(
        &mut on,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );
    apply(
        &mut off,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&on, GENERAL), 3);
    assert_eq!(mentions(&off, GENERAL), 2);
}

#[test]
fn mention_on_all_messages_skips_channels_that_dont_notify_everything() {
    let mut state = ready_where(|ready| {
        ready["notification_settings"]["flags"] = (16 | 32).into();
    });

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 2, "GENERAL is set to nothing");
}

#[test]
fn a_message_at_or_before_the_read_position_counts_nothing() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_001, GENERAL),
    );

    assert_eq!(mentions(&state, GENERAL), 2);
}

#[test]
fn message_ack_moves_the_position_and_sets_the_count() {
    let mut state = ready_state();
    let mut acked = ack(GENERAL, 400_000_000_000_000_003);
    acked["mention_count"] = 1.into();

    apply(&mut state, "MESSAGE_ACK", acked);

    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_003), 1))
    );
}

#[test]
fn an_older_ack_is_ignored_unless_manual() {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_001),
    );
    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );

    let mut manual = ack(GENERAL, 400_000_000_000_000_001);
    manual["manual"] = true.into();
    manual["mention_count"] = 5.into();
    apply(&mut state, "MESSAGE_ACK", manual);

    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_001), 5))
    );
}

#[test]
fn an_ack_without_a_count_clears_mentions_once_it_reaches_the_newest_message() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_002),
    );
    assert_eq!(
        mentions(&state, GENERAL),
        2,
        "not at the newest message yet"
    );
    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_003),
    );

    assert_eq!(mentions(&state, GENERAL), 0);
}

#[test]
fn an_ack_of_another_read_state_type_changes_nothing() {
    let mut state = ready_state();
    let mut other = ack(GENERAL, 400_000_000_000_000_003);
    other["ack_type"] = 1.into();

    apply(&mut state, "MESSAGE_ACK", other);

    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );
}

#[test]
fn an_ack_for_a_channel_without_a_read_state_creates_one() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GROUP, 400_000_000_000_000_021),
    );

    assert_eq!(
        read_state(&state, GROUP),
        Some((Some(400_000_000_000_000_021), 0))
    );
}

#[test]
fn channel_unread_update_moves_the_newest_message_forward_only() {
    let mut state = ready_state();
    let update = |last: u64| {
        json!({
            "guild_id": G1.to_string(),
            "channel_unread_updates": [{"id": GENERAL.to_string(), "last_message_id": last.to_string()}]
        })
    };

    apply(
        &mut state,
        "CHANNEL_UNREAD_UPDATE",
        update(400_000_000_000_000_005),
    );
    apply(
        &mut state,
        "CHANNEL_UNREAD_UPDATE",
        update(400_000_000_000_000_004),
    );

    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_005));
}

#[test]
fn a_single_passive_update_makes_a_read_channel_unread() {
    let mut state = general_read();

    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, GENERAL, 400_000_000_000_000_006),
    );

    assert!(unread(&state, GENERAL));
}

#[test]
fn a_channel_update_after_a_passive_update_keeps_the_channel_unread() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, GENERAL, 400_000_000_000_000_006),
    );

    for update in [
        json!({"id": GENERAL.to_string(), "last_message_id": "400000000000000003"}),
        json!({"id": GENERAL.to_string(), "last_message_id": null}),
        json!({"id": GENERAL.to_string(), "topic": "changed"}),
    ] {
        apply(&mut state, "CHANNEL_UPDATE", update);
    }

    assert!(unread(&state, GENERAL));
    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_006));
}

#[test]
fn an_older_passive_update_changes_nothing() {
    let mut state = ready_state();

    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, GENERAL, 400_000_000_000_000_001),
    );

    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_003));
    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );
}

#[test]
fn a_passive_update_for_an_unknown_channel_is_ignored() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, 300_000_000_000_000_099, 400_000_000_000_000_006),
    );
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G2, GENERAL, 400_000_000_000_000_006),
    );

    assert!(events.is_empty(), "{events:?}");
    assert!(
        state
            .channel(Snowflake::new(300_000_000_000_000_099))
            .is_none()
    );
    assert_eq!(
        last_message(&state, GENERAL),
        Some(400_000_000_000_000_003),
        "GENERAL isn't in G2"
    );
}

#[test]
fn an_ack_past_the_cursor_reads_the_channel() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, GENERAL, 400_000_000_000_000_006),
    );
    assert!(unread(&state, GENERAL));

    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_009),
    );

    assert!(!unread(&state, GENERAL));
    assert_eq!(mentions(&state, GENERAL), 0);
}

fn forum(last: Option<u64>) -> Value {
    json!({
        "id": FORUM.to_string(),
        "type": 15,
        "guild_id": G1.to_string(),
        "name": "ideas",
        "position": 3,
        "permission_overwrites": [],
        "last_message_id": last.map(|id| id.to_string())
    })
}

fn post(id: u64) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread["id"] = id.to_string().into();
    thread["parent_id"] = FORUM.to_string().into();
    thread
}

#[test]
fn a_new_forum_post_moves_the_forums_cursor() {
    let mut state = ready_state();
    apply(&mut state, "CHANNEL_CREATE", forum(None));
    assert!(!unread(&state, FORUM));

    apply(&mut state, "THREAD_CREATE", post(AFTER_JOIN + 100));
    apply(&mut state, "THREAD_CREATE", post(AFTER_JOIN + 50));

    assert_eq!(last_message(&state, FORUM), Some(AFTER_JOIN + 100));
    assert!(unread(&state, FORUM));
}

#[test]
fn a_thread_outside_a_forum_leaves_its_parent_alone() {
    let mut state = general_read();
    let mut thread = post(AFTER_JOIN + 100);
    thread["parent_id"] = GENERAL.to_string().into();

    apply(&mut state, "THREAD_CREATE", thread);

    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_003));
}

#[test]
fn a_thread_started_message_makes_the_parent_unread() {
    let mut state = general_read();
    let mut started = plain(400_000_000_000_000_007, GENERAL);
    started["type"] = 18.into();

    apply(&mut state, "MESSAGE_CREATE", started);

    assert!(unread(&state, GENERAL));
}

#[test]
fn a_message_in_an_unjoined_thread_leaves_the_parent_read() {
    let mut state = general_read();
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread.as_object_mut().unwrap().remove("member");
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_008, UNJOINED),
    );

    assert!(!unread(&state, GENERAL));
    assert_eq!(
        last_message(&state, UNJOINED),
        Some(400_000_000_000_000_008)
    );
}

#[test]
fn a_new_ready_replaces_the_read_states() {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GROUP, 400_000_000_000_000_021),
    );
    apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_003),
    );

    apply(&mut state, "READY", ready_value());

    assert_eq!(read_state(&state, GROUP), None);
    assert_eq!(
        read_state(&state, GENERAL),
        Some((Some(400_000_000_000_000_002), 2))
    );
}

#[test]
fn read_states_of_other_types_are_left_out() {
    let state = ready_state();

    assert_eq!(read_state(&state, G1), None);
}

#[test]
fn a_guild_create_after_a_passive_update_keeps_the_channel_unread() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(G1, GENERAL, 400_000_000_000_000_006),
    );
    let mut guild = ready_value()["guilds"][0].clone();
    guild["channels"][1]["last_message_id"] = "400000000000000003".into();

    apply(&mut state, "GUILD_CREATE", guild);

    assert!(unread(&state, GENERAL));
    assert_eq!(last_message(&state, GENERAL), Some(400_000_000_000_000_006));
}

#[test]
fn a_dm_opened_by_its_first_message_is_unread() {
    let mut state = ready_state();
    let mut opened = fixture(include_str!(
        "../../../../tests/fixtures/channel_create_dm.json"
    ));
    opened["last_message_id"] = "400000000000000040".into();
    apply(&mut state, "CHANNEL_CREATE", opened);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_plain(400_000_000_000_000_040, 300_000_000_000_000_013),
    );

    assert!(unread(&state, 300_000_000_000_000_013));
    assert_eq!(mentions(&state, 300_000_000_000_000_013), 1);
}
