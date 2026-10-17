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

fn thread_unread(ready: &mut Value) {
    read_general(ready);
    ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
}

fn mute_thread(ready: &mut Value) {
    let member = &mut ready["guilds"][0]["threads"][0]["member"];
    member["muted"] = true.into();
    member["mute_config"] = Value::Null;
}

fn mute_general(ready: &mut Value) {
    general_override(ready)["mute_config"] = Value::Null;
}

fn hide_muted(ready: &mut Value, hide: bool) {
    ready["user_guild_settings"]["entries"][0]["hide_muted_channels"] = hide.into();
}

#[test]
fn a_mention_past_the_window_keeps_the_guild_badge() {
    let state = ready_where(|ready| {
        read_general(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
    });

    assert!(state.listed(Snowflake::new(THREAD)), "past its window");
    assert_eq!(guild_badge(&state, G1), badge(false, 1));
}

#[test]
fn a_muted_thread_adds_no_dot_but_its_mentions() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
        mute_thread(ready);
        hide_muted(ready, false);
    });

    assert_eq!(channel_badge(&state, THREAD), badge(false, 1));
    assert_eq!(guild_badge(&state, G1), badge(false, 1));
}

#[test]
fn a_muted_channel_leaves_its_threads_alone() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        mute_general(ready);
        hide_muted(ready, false);
    });

    assert_eq!(channel_badge(&state, THREAD), badge(true, 0));
    assert_eq!(guild_badge(&state, G1), badge(true, 0));
}

#[test]
fn a_muted_category_leaves_its_threads_alone() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        ready["user_guild_settings"]["entries"][0]["channel_overrides"][0]["muted"] = true.into();
    });

    assert_eq!(channel_badge(&state, THREAD), badge(true, 0));
    assert_eq!(guild_badge(&state, G1), badge(true, 0));
}

#[test]
fn a_thread_under_a_parent_hidden_as_muted_still_counts() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
        mute_general(ready);
        hide_muted(ready, true);
    });

    assert_eq!(guild_badge(&state, G1), badge(true, 1));
}

#[test]
fn a_muted_thread_under_hide_muted_adds_nothing() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
        mute_thread(ready);
        hide_muted(ready, true);
    });

    assert_eq!(guild_badge(&state, G1), badge(false, 0));
}

#[test]
fn the_thread_level_changes_no_badge() {
    let state = ready_where(|ready| {
        thread_unread(ready);
        ready["guilds"][0]["threads"][0]["member"]["flags"] = (1 | 4).into();
    });

    assert_eq!(channel_badge(&state, THREAD), badge(true, 0));
}

#[test]
fn an_archived_threads_mentions_leave_the_guild_badge() {
    let mut state = ready_where(|ready| {
        thread_unread(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
    });
    assert_eq!(guild_badge(&state, G1), badge(true, 1));

    apply(
        &mut state,
        "THREAD_UPDATE",
        json!({
            "id": THREAD.to_string(), "guild_id": G1.to_string(), "type": 11,
            "thread_metadata": {"archived": true, "auto_archive_duration": 1440,
                                "archive_timestamp": "2026-10-08T12:00:00.000000+00:00", "locked": false}
        }),
    );

    assert!(!state.listed(Snowflake::new(THREAD)));
    assert_eq!(guild_badge(&state, G1), badge(false, 0));
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

fn requests_and_spam(ready: &mut Value) {
    let (group, dm) = (GROUP.to_string(), DM.to_string());
    for channel in ready["private_channels"].as_array_mut().unwrap() {
        let id = channel["id"].as_str().map(str::to_owned);
        if id.as_deref() == Some(group.as_str()) {
            channel["is_message_request"] = true.into();
        } else if id.as_deref() == Some(dm.as_str()) {
            channel["is_spam"] = true.into();
        }
    }
}

#[test]
fn message_requests_and_spam_stay_off_the_rail() {
    let mut state = ready_where(requests_and_spam);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_022, GROUP),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_023, DM),
    );

    assert!(state.unread_private_channels().is_empty());
    assert_eq!(
        channel_badge(&state, GROUP).mentions,
        1,
        "the row's own badge stays"
    );
}

#[test]
fn an_accepted_message_request_comes_back() {
    let mut state = ready_where(requests_and_spam);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_022, GROUP),
    );

    let events = apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GROUP.to_string(), "is_message_request": false}),
    );

    assert!(
        events.contains(&format!("ChannelUpdated({GROUP})")),
        "{events:?}"
    );
    assert_eq!(
        channel_ids_in_order(state.unread_private_channels()),
        [GROUP]
    );
}

fn not_an_admin(ready: &mut Value) {
    ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
    ready["merged_members"][0][0]["roles"] = json!([]);
    read_general(ready);
}

fn hide_general() -> Value {
    json!({
        "id": GENERAL.to_string(),
        "permission_overwrites": [{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]
    })
}

#[test]
fn hiding_a_channel_also_hides_its_threads() {
    let mut state = ready_where(|ready| {
        not_an_admin(ready);
        ready["read_state"]["entries"][1]["mention_count"] = 1.into();
    });
    assert_eq!(channel_badge(&state, THREAD), badge(false, 1));

    let events = apply(&mut state, "CHANNEL_UPDATE", hide_general());

    assert_eq!(channel_badge(&state, THREAD), Badge::default());
    assert!(
        events.contains(&format!("BadgeChanged({THREAD})")),
        "{events:?}"
    );
}

#[test]
fn a_thread_created_later_follows_its_parent() {
    let mut state = ready_where(not_an_admin);
    apply(
        &mut state,
        "THREAD_CREATE",
        fixture(include_str!(
            "../../../../tests/fixtures/thread_create.json"
        )),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        message(AFTER_JOIN + 9, 300_000_000_000_000_021),
    );
    assert_eq!(
        channel_badge(&state, 300_000_000_000_000_021),
        badge(true, 1)
    );

    apply(&mut state, "CHANNEL_UPDATE", hide_general());

    assert_eq!(
        channel_badge(&state, 300_000_000_000_000_021),
        Badge::default()
    );
}

const OPT_IN_CHANNELS_ON: u64 = 1 << 14;
const OPT_IN_ENABLED: u64 = 1 << 12;

// A community server with "Channels & Roles" opt-in on, as the user's settings say.
fn opt_in(ready: &mut Value) {
    ready["guilds"][0]["properties"]["features"] = json!(["COMMUNITY"]);
    let settings = &mut ready["user_guild_settings"]["entries"][0];
    let flags = settings["flags"].as_u64().unwrap();
    settings["flags"] = (flags | OPT_IN_CHANNELS_ON).into();
}

fn opt_into(override_: &mut Value) {
    let flags = override_["flags"].as_u64().unwrap_or(0);
    override_["flags"] = (flags | OPT_IN_ENABLED).into();
}

// GENERAL's newest message a day old, past its read position.
fn recent_general(ready: &mut Value) {
    opt_in(ready);
    let day_ago =
        Snowflake::<model::GenericMarker>::from_unix_millis(wall_millis() - 86_400_000, 0);
    ready["guilds"][0]["channels"][1]["last_message_id"] = day_ago.get().to_string().into();
}

#[test]
fn an_opt_in_server_shows_nothing_for_a_channel_quiet_for_a_week() {
    // The fixture's GENERAL has two old mentions and a newest message from 2018.
    let state = ready_where(opt_in);

    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert_eq!(guild_badge(&state, G1), Badge::default());
}

#[test]
fn opt_in_needs_a_community_server() {
    let state = ready_where(|ready| {
        opt_in(ready);
        ready["guilds"][0]["properties"]["features"] = json!([]);
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 2));
}

#[test]
fn a_recent_channel_the_user_opted_into_shows_unread() {
    let state = ready_where(|ready| {
        recent_general(ready);
        ready["read_state"]["entries"][0]["mention_count"] = 0.into();
        opt_into(general_override(ready));
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 0));
    assert_eq!(guild_badge(&state, G1), badge(true, 0));
}

#[test]
fn an_opted_in_category_covers_its_channels() {
    let state = ready_where(|ready| {
        recent_general(ready);
        ready["read_state"]["entries"][0]["mention_count"] = 0.into();
        opt_into(&mut ready["user_guild_settings"]["entries"][0]["channel_overrides"][0]);
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 0));
}

#[test]
fn a_channel_not_opted_into_shows_only_with_mentions() {
    let quiet = ready_where(|ready| {
        recent_general(ready);
        ready["read_state"]["entries"][0]["mention_count"] = 0.into();
    });
    let pinged = ready_where(recent_general);

    assert_eq!(channel_badge(&quiet, GENERAL), Badge::default());
    assert_eq!(guild_badge(&quiet, G1), Badge::default());
    assert_eq!(channel_badge(&pinged, GENERAL), badge(true, 2));
}

const IS_GUILD_RESOURCE_CHANNEL: u64 = 1 << 7;

fn community(ready: &mut Value) {
    ready["guilds"][0]["properties"]["features"] = json!(["COMMUNITY"]);
}

// A Server Guide resource channel.
fn resource(ready: &mut Value) {
    let channel = &mut ready["guilds"][0]["channels"][1];
    let flags = channel["flags"].as_u64().unwrap_or(0);
    channel["flags"] = (flags | IS_GUILD_RESOURCE_CHANNEL).into();
}

// GENERAL's newest message `days` old, past its read position.
fn general_last_message(ready: &mut Value, days: i64) {
    let then =
        Snowflake::<model::GenericMarker>::from_unix_millis(wall_millis() - days * 86_400_000, 0);
    ready["guilds"][0]["channels"][1]["last_message_id"] = then.get().to_string().into();
}

// Manual check B2 on #21: Discord reported this channel unread with an @everyone mention, and
// the official client showed it read without ever acking it.
#[test]
fn a_server_guide_resource_channel_shows_nothing() {
    let state = ready_where(|ready| {
        community(ready);
        resource(ready);
        general_last_message(ready, 19);
        ready["read_state"]["entries"][0]["mention_count"] = 1.into();
    });

    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert_eq!(guild_badge(&state, G1), Badge::default());
}

#[test]
fn a_resource_channel_shows_nothing_in_any_server_however_recent() {
    let state = ready_where(|ready| {
        resource(ready);
        general_last_message(ready, 0);
    });

    assert_eq!(channel_badge(&state, GENERAL), Badge::default());
    assert_eq!(guild_badge(&state, G1), Badge::default());
}

#[test]
fn without_opt_in_a_quiet_community_channel_keeps_its_mention() {
    let state = ready_where(|ready| {
        community(ready);
        general_last_message(ready, 19);
        ready["read_state"]["entries"][0]["mention_count"] = 1.into();
    });

    assert_eq!(channel_badge(&state, GENERAL), badge(true, 1));
    assert_eq!(guild_badge(&state, G1).mentions, 1);
}
