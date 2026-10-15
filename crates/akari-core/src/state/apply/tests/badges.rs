use super::*;
use crate::state::Badge;

const CATEGORY: u64 = 300_000_000_000_000_001;
const ROLE: u64 = 500_000_000_000_000_002;
const FORUM: u64 = 300_000_000_000_000_030;
const AFTER_JOIN: u64 = 1_300_000_000_000_000_000;

fn badge(unread: bool, mentions: u32) -> Badge {
    Badge { unread, mentions }
}

fn channel_badge(state: &State, channel: u64) -> Badge {
    state.channel_badge(Snowflake::new(channel))
}

fn guild_badge(state: &State, guild: u64) -> Badge {
    state.guild_badge(Snowflake::new(guild))
}

fn ready_where(edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    edit(&mut ready);
    state_from(&ready)
}

fn badge_events(events: &[String]) -> Vec<String> {
    events
        .iter()
        .filter(|event| event.contains("BadgeChanged"))
        .cloned()
        .collect()
}

fn ack(channel: u64, message: u64) -> Value {
    json!({"channel_id": channel.to_string(), "message_id": message.to_string(), "version": 1})
}

fn plain(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["mentions"] = json!([]);
    message
}

fn read_general(ready: &mut Value) {
    ready["read_state"]["entries"][0]["last_message_id"] = "400000000000000003".into();
    ready["read_state"]["entries"][0]["mention_count"] = 0.into();
}

fn general_override(ready: &mut Value) -> &mut Value {
    &mut ready["user_guild_settings"]["entries"][0]["channel_overrides"][1]
}

// G1's settings from the fixture, changed by `edit`, as USER_GUILD_SETTINGS_UPDATE.
fn g1_settings(edit: impl FnOnce(&mut Value)) -> Value {
    let mut settings = ready_value()["user_guild_settings"]["entries"][0].clone();
    edit(&mut settings);
    settings
}

#[test]
fn a_guild_is_unread_when_one_counted_channel_is() {
    let mut state = ready_state();
    assert_eq!(channel_badge(&state, GENERAL), badge(true, 2));
    assert_eq!(guild_badge(&state, G1), badge(true, 2));
    assert_eq!(guild_badge(&state, G2), badge(false, 0));

    let mut acked = ack(GENERAL, 400_000_000_000_000_003);
    acked["mention_count"] = 0.into();
    apply(&mut state, "MESSAGE_ACK", acked);

    assert_eq!(channel_badge(&state, GENERAL), badge(false, 0));
    assert_eq!(guild_badge(&state, G1), badge(false, 0));
}

#[test]
fn a_muted_guild_shows_no_dot_but_its_mentions() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "USER_GUILD_SETTINGS_UPDATE",
        g1_settings(|settings| settings["muted"] = true.into()),
    );

    assert_eq!(guild_badge(&state, G1), badge(false, 2));
    assert_eq!(
        channel_badge(&state, GENERAL),
        badge(true, 2),
        "rows keep their look"
    );
    assert_eq!(
        badge_events(&events),
        ["GuildBadgeChanged(200000000000000001)"]
    );
}

#[test]
fn a_muted_channel_adds_no_dot_but_its_mentions() {
    let state = ready_where(|ready| {
        general_override(ready)["mute_config"] = Value::Null;
        ready["user_guild_settings"]["entries"][0]["hide_muted_channels"] = false.into();
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(false, 2));
    assert_eq!(guild_badge(&state, G1), badge(false, 2));
}

#[test]
fn a_channel_under_a_muted_category_looks_unread_but_adds_no_dot() {
    let state = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["channel_overrides"][0]["muted"] = true.into();
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 2));
    assert_eq!(guild_badge(&state, G1), badge(false, 2));
}

#[test]
fn a_channel_the_user_cant_view_counts_nothing() {
    let state = ready_where(|ready| {
        ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
        ready["merged_members"][0][0]["roles"] = json!([]);
        ready["guilds"][0]["channels"][1]["permission_overwrites"] =
            json!([{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]);
    });

    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert_eq!(guild_badge(&state, G1), Badge::default());
}

#[test]
fn a_role_change_that_hides_a_channel_updates_the_badge() {
    let mut state = ready_where(|ready| {
        ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
        ready["guilds"][0]["channels"][1]["permission_overwrites"] =
            json!([{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]);
    });
    assert_eq!(
        guild_badge(&state, G1),
        badge(true, 2),
        "the role is an admin"
    );

    let events = apply(
        &mut state,
        "GUILD_ROLE_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "role": {"id": ROLE.to_string(), "name": "crew", "permissions": "0", "position": 1,
                     "color": 0, "hoist": false, "managed": false, "mentionable": true, "flags": 0}
        }),
    );

    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert_eq!(guild_badge(&state, G1), Badge::default());
    assert!(
        events.contains(&"BadgeChanged(300000000000000002)".to_owned()),
        "{events:?}"
    );
    assert!(
        events.contains(&"GuildBadgeChanged(200000000000000001)".to_owned()),
        "{events:?}"
    );
}

#[test]
fn a_voice_channels_chat_never_shows_unread_but_counts_mentions() {
    let state =
        ready_where(|ready| {
            read_general(ready);
            ready["guilds"][0]["channels"][2]["last_message_id"] = "400000000000000002".into();
            ready["read_state"]["entries"].as_array_mut().unwrap().push(json!({
            "id": VOICE.to_string(), "last_message_id": "400000000000000001", "mention_count": 1
        }));
        });

    assert_eq!(channel_badge(&state, VOICE), badge(false, 1));
    assert_eq!(guild_badge(&state, G1), badge(false, 1));
}

#[test]
fn joined_threads_count_for_the_guild() {
    let state = ready_where(|ready| {
        read_general(ready);
        ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
    });

    assert_eq!(channel_badge(&state, THREAD), badge(true, 1));
    assert_eq!(guild_badge(&state, G1), badge(true, 1));
}

#[test]
fn an_unjoined_threads_messages_give_the_server_no_dot() {
    let mut state = ready_where(read_general);
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread.as_object_mut().unwrap().remove("member");
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_008, 300_000_000_000_000_021),
    );

    assert_eq!(guild_badge(&state, G1), badge(false, 0));
}

// Being mentioned adds the user to a thread, which Akari doesn't hear about until READY.
#[test]
fn a_ping_in_an_unjoined_thread_reaches_the_server_badge() {
    let mut state = ready_where(read_general);
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread.as_object_mut().unwrap().remove("member");
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(AFTER_JOIN + 8, 300_000_000_000_000_021),
    );

    assert_eq!(guild_badge(&state, G1), badge(false, 1));
}

#[test]
fn an_archived_thread_doesnt_count() {
    let mut state = ready_where(|ready| {
        read_general(ready);
        ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
    });
    assert_eq!(guild_badge(&state, G1), badge(true, 0));

    apply(
        &mut state,
        "THREAD_UPDATE",
        json!({
            "id": THREAD.to_string(), "guild_id": G1.to_string(), "type": 11,
            "thread_metadata": {"archived": true, "auto_archive_duration": 1440,
                                "archive_timestamp": "2026-10-08T12:00:00.000000+00:00", "locked": false}
        }),
    );

    assert_eq!(guild_badge(&state, G1), badge(false, 0));
}

#[test]
fn hidden_muted_channels_dont_count() {
    let hiding = ready_where(|ready| general_override(ready)["mute_config"] = Value::Null);
    let showing = ready_where(|ready| {
        general_override(ready)["mute_config"] = Value::Null;
        ready["user_guild_settings"]["entries"][0]["hide_muted_channels"] = false.into();
    });

    assert_eq!(guild_badge(&hiding, G1), badge(false, 0));
    assert_eq!(guild_badge(&showing, G1), badge(false, 2));
    assert_eq!(channel_badge(&hiding, GENERAL), badge(false, 2));
}

#[test]
fn a_forum_post_makes_the_server_unread() {
    let mut state = ready_where(|ready| {
        read_general(ready);
        ready["notification_settings"]["flags"] = 0.into();
        ready["user_guild_settings"]["entries"][0]["flags"] = 0.into();
    });
    apply(
        &mut state,
        "CHANNEL_CREATE",
        json!({"id": FORUM.to_string(), "type": 15, "guild_id": G1.to_string(), "name": "ideas",
               "position": 3, "permission_overwrites": [], "last_message_id": null}),
    );
    assert_eq!(guild_badge(&state, G1), badge(false, 0));
    let mut post = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    post["id"] = AFTER_JOIN.to_string().into();
    post["parent_id"] = FORUM.to_string().into();

    let events = apply(&mut state, "THREAD_CREATE", post);

    assert_eq!(channel_badge(&state, FORUM), badge(true, 0));
    assert_eq!(guild_badge(&state, G1), badge(true, 0));
    assert!(
        events.contains(&"GuildBadgeChanged(200000000000000001)".to_owned()),
        "{events:?}"
    );
}

#[test]
fn only_mentions_shows_no_dot_with_new_notifications() {
    let state = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["flags"] = 0.into();
        general_override(ready)["flags"] = 0.into();
        general_override(ready)["message_notifications"] = 3.into();
    });

    assert_eq!(
        channel_badge(&state, GENERAL),
        badge(false, 2),
        "G1 is set to mentions"
    );
    assert_eq!(guild_badge(&state, G1), badge(false, 2));
}

#[test]
fn the_unread_flags_win_over_the_level() {
    let channel_flag = ready_state();
    let guild_flag = ready_where(|ready| {
        general_override(ready)["flags"] = 0.into();
        general_override(ready)["message_notifications"] = 0.into();
    });

    assert_eq!(
        channel_badge(&channel_flag, GENERAL),
        badge(true, 2),
        "the channel's ALL_MESSAGES flag over G1's level and flag"
    );
    assert_eq!(
        channel_badge(&guild_flag, GENERAL),
        badge(false, 2),
        "G1's ONLY_MENTIONS flag over the channel's level"
    );
}

#[test]
fn without_new_notifications_every_unread_shows() {
    let state = ready_where(|ready| {
        ready["notification_settings"]["flags"] = 0.into();
        ready["user_guild_settings"]["entries"][0]["flags"] = 0.into();
        general_override(ready)["flags"] = 0.into();
        general_override(ready)["message_notifications"] = 2.into();
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 2));
}

#[test]
fn a_message_in_an_already_unread_channel_sends_nothing() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(badge_events(&events), Vec::<String>::new());
}

#[test]
fn the_first_unread_message_sends_both_events() {
    let mut state = ready_where(read_general);

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(400_000_000_000_000_004, GENERAL),
    );

    assert_eq!(
        badge_events(&events),
        [
            "BadgeChanged(300000000000000002)",
            "GuildBadgeChanged(200000000000000001)"
        ]
    );
}

#[test]
fn badge_changed_names_the_channels_guild() {
    let mut state = ready_state();
    let mut events = Vec::new();

    state.apply(
        dispatch("MESSAGE_ACK", &ack(GENERAL, 400_000_000_000_000_003)),
        &mut events,
    );

    assert!(
        events.iter().any(|event| matches!(
            event,
            StoreEvent::BadgeChanged { channel_id, guild_id }
                if channel_id.get() == GENERAL && guild_id.map(|id| id.get()) == Some(G1)
        )),
        "{events:?}"
    );
}

#[test]
fn a_read_state_change_alone_sends_badge_changed() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "MESSAGE_ACK",
        ack(GENERAL, 400_000_000_000_000_002),
    );
    assert_eq!(
        badge_events(&events),
        Vec::<String>::new(),
        "the same position"
    );
    let mut manual = ack(GENERAL, 400_000_000_000_000_001);
    manual["manual"] = true.into();
    manual["mention_count"] = 2.into();
    let events = apply(&mut state, "MESSAGE_ACK", manual);

    assert_eq!(badge_events(&events), ["BadgeChanged(300000000000000002)"]);
}

#[test]
fn a_mute_ending_restores_the_dot() {
    let end = 1_900_000_000_000;
    let mut state = ready_where(|ready| {
        ready["user_guild_settings"]["entries"][0]["muted"] = true.into();
        ready["user_guild_settings"]["entries"][0]["mute_config"] =
            json!({"end_time": "2030-03-17T17:46:40.000000+00:00", "selected_time_window": 3600});
    });
    assert_eq!(guild_badge(&state, G1), badge(false, 2));

    let mut events = Vec::new();
    state.end_mutes(&[Some(Snowflake::new(G1))], end, &mut events);

    assert_eq!(guild_badge(&state, G1), badge(true, 2));
    let events: Vec<String> = events.iter().map(describe).collect();
    assert!(
        events.contains(&"GuildBadgeChanged(200000000000000001)".to_owned()),
        "{events:?}"
    );
}

#[test]
fn a_later_ready_reports_only_badges_that_changed() {
    let mut state = ready_state();
    let mut ready = ready_value();
    read_general(&mut ready);

    let events = apply(&mut state, "READY", ready);

    assert_eq!(
        badge_events(&events),
        [
            "BadgeChanged(300000000000000002)",
            "GuildBadgeChanged(200000000000000001)"
        ]
    );
}

#[test]
fn deleting_a_channel_removes_its_part_of_the_guild_badge() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": GENERAL.to_string(), "guild_id": G1.to_string(), "type": 0}),
    );

    assert_eq!(guild_badge(&state, G1), Badge::default());
    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert!(
        events.contains(&"GuildBadgeChanged(200000000000000001)".to_owned()),
        "{events:?}"
    );
}

#[test]
fn a_removed_guild_has_no_badge() {
    let mut state = ready_state();

    apply(&mut state, "GUILD_DELETE", json!({"id": G1.to_string()}));

    assert_eq!(guild_badge(&state, G1), Badge::default());
}

#[test]
fn a_muted_dm_shows_no_dot_but_its_mentions() {
    let mut state = ready_state();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_022, DM),
    );
    let mut plain_group = dm_message(400_000_000_000_000_023, GROUP);
    plain_group["mentions"] = json!([]);
    apply(&mut state, "MESSAGE_CREATE", plain_group);

    assert_eq!(channel_badge(&state, DM), badge(false, 1));
    assert_eq!(channel_badge(&state, GROUP), badge(true, 1));
}

#[test]
fn unread_private_channels_are_dms_with_mentions_newest_first() {
    let mut state = ready_state();
    assert!(state.unread_private_channels().is_empty());
    let mut plain_dm = dm_message(400_000_000_000_000_022, DM);
    plain_dm["mentions"] = json!([]);
    apply(&mut state, "MESSAGE_CREATE", plain_dm);
    assert!(
        state.unread_private_channels().is_empty(),
        "a plain message in a muted DM"
    );

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_023, GROUP),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_024, DM),
    );

    assert_eq!(
        channel_ids_in_order(state.unread_private_channels()),
        [DM, GROUP]
    );
}

fn channel_ids_in_order(channels: Vec<Arc<Channel>>) -> Vec<u64> {
    channels.iter().map(|channel| channel.id.get()).collect()
}

#[test]
fn categories_have_no_badge() {
    let state = ready_state();

    assert_eq!(channel_badge(&state, CATEGORY), Badge::default());
}
