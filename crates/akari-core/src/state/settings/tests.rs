use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::*;
use crate::gateway::settings_proto::encode::{Wire, field};
use crate::gateway::{DispatchEvent, GatewayEvent, decode};
use crate::model::Snowflake;
use crate::state::apply::State;
use crate::state::{DEFAULT_LIMITS, Store, StoreEvent};

const G1: u64 = 200_000_000_000_000_001;
const G2: u64 = 200_000_000_000_000_002;
const G3: u64 = 200_000_000_000_000_003;
const G4: u64 = 200_000_000_000_000_004;
const UNKNOWN: u64 = 200_000_000_000_000_009;
const CATEGORY: u64 = 300_000_000_000_000_001;
const GENERAL: u64 = 300_000_000_000_000_002;
const G3_WELCOME: u64 = 300_000_000_000_000_031;
const DM: u64 = 300_000_000_000_000_010;
const GROUP: u64 = 300_000_000_000_000_011;
// 2026-10-01T12:00:00Z, the fixture's ended channel mute.
const ENDED: i64 = 1_790_856_000_000;
const LATER: &str = "2099-01-01T00:00:00.000000+00:00";

fn dispatch(name: &str, data: &Value) -> DispatchEvent {
    let payload = json!({"op": 0, "s": 1, "t": name, "d": data});
    match decode(payload.to_string().as_bytes()) {
        Ok(GatewayEvent::Dispatch { event, .. }) => event,
        other => panic!("expected a dispatch, got {other:?}"),
    }
}

fn ready_value() -> Value {
    let ready: Value = serde_json::from_str(include_str!("../../../tests/fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"].clone()
}

fn guild_create(id: u64, channel: u64, joined_at: &str) -> Value {
    let mut guild: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/guild_create.json"))
            .unwrap_or_else(|err| panic!("{err}"));
    guild["id"] = id.to_string().into();
    guild["properties"]["id"] = id.to_string().into();
    guild["channels"][0]["id"] = channel.to_string().into();
    guild["members"][0]["joined_at"] = joined_at.into();
    guild
}

struct Entry {
    id: Option<i64>,
    name: Option<&'static str>,
    color: Option<u64>,
    guilds: Vec<u64>,
}

fn plain(guilds: &[u64]) -> Entry {
    Entry {
        id: None,
        name: None,
        color: None,
        guilds: guilds.to_vec(),
    }
}

fn folder(id: i64, guilds: &[u64]) -> Entry {
    Entry {
        id: Some(id),
        ..plain(guilds)
    }
}

fn folders_field(entries: &[Entry]) -> Vec<u8> {
    let entries: Vec<Vec<u8>> = entries
        .iter()
        .map(|entry| {
            let mut bytes = field(
                1,
                Wire::Bytes(
                    entry
                        .guilds
                        .iter()
                        .flat_map(|id| id.to_le_bytes())
                        .collect(),
                ),
            );
            if let Some(id) = entry.id {
                bytes.extend(field(2, Wire::Bytes(field(1, Wire::Varint(id as u64)))));
            }
            if let Some(name) = entry.name {
                let name = field(1, Wire::Bytes(name.as_bytes().to_vec()));
                bytes.extend(field(3, Wire::Bytes(name)));
            }
            if let Some(color) = entry.color {
                bytes.extend(field(4, Wire::Bytes(field(1, Wire::Varint(color)))));
            }
            field(1, Wire::Bytes(bytes))
        })
        .collect();
    field(14, Wire::Bytes(entries.concat()))
}

fn proto(entries: &[Entry]) -> String {
    STANDARD.encode(folders_field(entries))
}

fn versions_only() -> String {
    STANDARD.encode(field(1, Wire::Bytes(field(3, Wire::Varint(9)))))
}

fn proto_update(proto: &str, partial: bool) -> Value {
    json!({"settings": {"type": 1, "proto": proto}, "partial": partial})
}

// Only READY's guilds: G1 available, G2 unavailable.
fn ready_with(proto: &str) -> State {
    let mut ready = ready_value();
    ready["user_settings_proto"] = proto.into();
    let mut state = State::new();
    state.apply(dispatch("READY", &ready), &mut Vec::new());
    state
}

// READY with `proto`, then G3 (joined 2026-10-08) and G4 (joined 2026-10-09).
fn state_with(proto: &str) -> State {
    let mut state = ready_with(proto);
    for guild in [
        guild_create(G3, G3_WELCOME, "2026-10-08T12:00:00.000000+00:00"),
        guild_create(
            G4,
            300_000_000_000_000_041,
            "2026-10-09T12:00:00.000000+00:00",
        ),
    ] {
        state.apply(dispatch("GUILD_CREATE", &guild), &mut Vec::new());
    }
    state
}

fn apply(state: &mut State, name: &str, data: &Value) -> Vec<String> {
    let mut events = Vec::new();
    state.apply(dispatch(name, data), &mut events);
    events.iter().map(describe).collect()
}

fn describe(event: &StoreEvent) -> String {
    match event {
        StoreEvent::GuildAdded(guild) => format!("GuildAdded({})", guild.id.get()),
        StoreEvent::GuildUpdated(guild) => format!("GuildUpdated({})", guild.id.get()),
        StoreEvent::GuildRemoved { guild_id } => format!("GuildRemoved({})", guild_id.get()),
        StoreEvent::ServerListChanged => "ServerListChanged".to_owned(),
        StoreEvent::GuildSettingsChanged { guild_id } => format!(
            "GuildSettingsChanged({})",
            guild_id.map_or(0, |id| id.get())
        ),
        StoreEvent::Ready => "Ready".to_owned(),
        other => format!("{other:?}"),
    }
}

fn items(state: &State) -> Vec<String> {
    state
        .server_list()
        .iter()
        .map(|item| match item {
            ServerListItem::Guild(id) => id.get().to_string(),
            ServerListItem::Folder(folder) => format!(
                "folder {}: {}",
                folder.id,
                folder
                    .guilds
                    .iter()
                    .map(|id| id.get().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
        .collect()
}

fn s(id: u64) -> String {
    id.to_string()
}

fn guild_settings_update(guild: u64, overrides: Value) -> Value {
    json!({
        "guild_id": guild.to_string(),
        "muted": false,
        "mute_config": null,
        "message_notifications": 3,
        "suppress_everyone": false,
        "suppress_roles": false,
        "hide_muted_channels": false,
        "flags": 0,
        "version": 9,
        "channel_overrides": overrides,
    })
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

// The server list

#[test]
fn the_server_list_follows_the_folders() {
    let state = state_with(&proto(&[
        plain(&[G3]),
        Entry {
            name: Some("Games"),
            color: Some(0x5865f2),
            ..folder(7, &[G1, G4])
        },
    ]));

    assert_eq!(items(&state), [s(G3), format!("folder 7: {G1}, {G4}")]);
    let ServerListItem::Folder(games) = &state.server_list()[1] else {
        panic!("expected a folder");
    };
    assert_eq!(games.name.as_deref(), Some("Games"));
    assert_eq!(games.color, Some(0x5865f2));
}

#[test]
fn guilds_the_folders_dont_place_come_first_newest_join_first() {
    let state = state_with(&proto(&[plain(&[G1])]));

    assert_eq!(items(&state), [s(G4), s(G3), s(G1)]);
}

#[test]
fn folders_skip_unknown_and_unavailable_guilds() {
    let state = state_with(&proto(&[
        folder(7, &[UNKNOWN, G2, G1]),
        plain(&[G2]),
        plain(&[G3, G4]),
    ]));

    assert_eq!(items(&state), [format!("folder 7: {G1}"), s(G3), s(G4)]);
}

#[test]
fn a_folder_whose_guilds_are_gone_disappears() {
    let state = state_with(&proto(&[folder(7, &[UNKNOWN]), plain(&[G1, G3, G4])]));

    assert_eq!(items(&state), [s(G1), s(G3), s(G4)]);
}

#[test]
fn a_folder_without_an_id_lists_its_guilds_plainly() {
    let state = state_with(&proto(&[plain(&[G1, G4, G3])]));

    assert_eq!(items(&state), [s(G1), s(G4), s(G3)]);
}

#[test]
fn a_guild_listed_twice_shows_once() {
    let state = state_with(&proto(&[plain(&[G1]), folder(7, &[G1, G3]), plain(&[G4])]));

    assert_eq!(items(&state), [s(G1), format!("folder 7: {G3}"), s(G4)]);
}

#[test]
fn an_empty_name_and_an_impossible_color_are_none() {
    let state = state_with(&proto(&[Entry {
        name: Some(""),
        color: Some(0x1_000_000),
        ..folder(7, &[G1, G3, G4])
    }]));

    let ServerListItem::Folder(folder) = &state.server_list()[0] else {
        panic!("expected a folder");
    };
    assert_eq!(folder.name, None);
    assert_eq!(folder.color, None);
}

#[test]
fn without_folders_the_order_is_the_join_order() {
    let state = state_with("");

    assert_eq!(items(&state), [s(G4), s(G3), s(G1)]);
}

#[test]
fn guild_list_is_the_server_list_flattened() {
    let mut ready = ready_value();
    ready["user_settings_proto"] = proto(&[plain(&[G3]), folder(7, &[G1, G4])]).into();
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(dispatch("READY", &ready));
    for guild in [
        guild_create(G3, G3_WELCOME, "2026-10-08T12:00:00.000000+00:00"),
        guild_create(
            G4,
            300_000_000_000_000_041,
            "2026-10-09T12:00:00.000000+00:00",
        ),
    ] {
        store.apply(dispatch("GUILD_CREATE", &guild));
    }

    let ids: Vec<u64> = store
        .guild_list()
        .iter()
        .map(|guild| guild.id.get())
        .collect();
    assert_eq!(ids, [G3, G1, G4]);
    assert_eq!(store.server_list().len(), 2);
}

// Guild settings

#[test]
fn ready_settings_reach_the_store() {
    let state = state_with("");

    let lab = state.guild_settings(Some(Snowflake::new(G1)));
    assert!(!lab.muted);
    assert_eq!(lab.level, Some(NotificationLevel::OnlyMentions));
    assert_eq!(lab.unreads, Some(Unreads::OnlyMentions));
    assert!(lab.suppress_everyone);
    assert!(!lab.suppress_roles);
    assert!(lab.hide_muted_channels);
    let category = lab.channels[&Snowflake::new(CATEGORY)];
    assert_eq!(category.level, None);
    assert!(!category.muted);
    let general = lab.channels[&Snowflake::new(GENERAL)];
    assert!(general.muted);
    assert_eq!(general.mute_ends, Some(at(ENDED)));
    assert_eq!(general.level, Some(NotificationLevel::Nothing));
    assert_eq!(general.unreads, Some(Unreads::AllMessages));

    let dms = state.guild_settings(None);
    assert!(dms.channels[&Snowflake::new(DM)].muted);
}

#[test]
fn a_guild_without_an_entry_has_its_default_level() {
    let state = state_with("");

    assert_eq!(
        *state.guild_settings(Some(Snowflake::new(G3))),
        GuildSettings::default()
    );
    assert_eq!(
        state
            .guild(Snowflake::new(G3))
            .map(|guild| guild.default_message_notifications),
        Some(NotificationLevel::OnlyMentions)
    );
    assert_eq!(
        state.notification_level(Snowflake::new(G3_WELCOME)),
        NotificationLevel::OnlyMentions
    );
}

#[test]
fn dms_without_an_entry_are_not_muted() {
    let mut ready = ready_value();
    let entries = ready["user_guild_settings"]["entries"]
        .as_array_mut()
        .unwrap();
    entries.retain(|entry| !entry["guild_id"].is_null());
    let mut state = State::new();
    state.apply(dispatch("READY", &ready), &mut Vec::new());

    assert_eq!(*state.guild_settings(None), GuildSettings::default());
    assert!(!state.channel_muted_at(Snowflake::new(DM), at(ENDED)));
    assert_eq!(
        state.notification_level(Snowflake::new(GROUP)),
        NotificationLevel::AllMessages
    );
}

#[test]
fn a_timed_mute_is_muted_until_it_ends() {
    let settings = GuildSettings {
        muted: true,
        mute_ends: Some(at(ENDED)),
        ..GuildSettings::default()
    };

    assert!(settings.muted_now(at(ENDED - 1)));
    assert!(!settings.muted_now(at(ENDED)));
    assert!(!GuildSettings::default().muted_now(at(ENDED - 1)));
}

#[test]
fn a_mute_without_an_end_lasts() {
    let settings = ChannelSettings {
        muted: true,
        ..ChannelSettings::default()
    };

    assert!(settings.muted_now(at(i64::from(i32::MAX) * 1000)));
}

#[test]
fn an_ended_mute_from_ready_is_over() {
    let state = state_with("");

    assert!(state.channel_muted_at(Snowflake::new(GENERAL), at(ENDED - 1)));
    assert!(!state.channel_muted_at(Snowflake::new(GENERAL), at(ENDED)));
    assert!(state.channel_muted_at(Snowflake::new(DM), at(ENDED)));
}

#[test]
fn a_channel_under_a_muted_category_isnt_muted() {
    let mut state = state_with("");
    apply(
        &mut state,
        "USER_GUILD_SETTINGS_UPDATE",
        &guild_settings_update(
            G1,
            json!([{
                "channel_id": CATEGORY.to_string(),
                "collapsed": false,
                "message_notifications": 3,
                "muted": true,
                "mute_config": {"end_time": null, "selected_time_window": -1},
            }]),
        ),
    );

    assert!(state.channel_muted_at(Snowflake::new(CATEGORY), at(ENDED)));
    assert!(!state.channel_muted_at(Snowflake::new(GENERAL), at(ENDED)));
}

#[test]
fn a_muted_guild_leaves_its_channels_unmuted() {
    let mut state = state_with("");
    let mut update = guild_settings_update(G1, json!([]));
    update["muted"] = true.into();
    update["mute_config"] = json!({"end_time": LATER, "selected_time_window": 900});
    apply(&mut state, "USER_GUILD_SETTINGS_UPDATE", &update);

    assert!(
        state
            .guild_settings(Some(Snowflake::new(G1)))
            .muted_now(at(ENDED))
    );
    assert!(!state.channel_muted_at(Snowflake::new(GENERAL), at(ENDED)));
}

#[test]
fn levels_inherit_channel_category_guild_default() {
    let mut state = state_with("");
    let level = |state: &State| state.notification_level(Snowflake::new(GENERAL));
    let set = |state: &mut State, guild: u8, category: u8, channel: u8| {
        let mut update = guild_settings_update(
            G1,
            json!([
                {"channel_id": CATEGORY.to_string(), "message_notifications": category, "muted": false},
                {"channel_id": GENERAL.to_string(), "message_notifications": channel, "muted": false},
            ]),
        );
        update["message_notifications"] = guild.into();
        apply(state, "USER_GUILD_SETTINGS_UPDATE", &update);
    };

    set(&mut state, 3, 2, 0);
    assert_eq!(level(&state), NotificationLevel::AllMessages);
    set(&mut state, 3, 2, 3);
    assert_eq!(level(&state), NotificationLevel::Nothing);
    set(&mut state, 0, 3, 3);
    assert_eq!(level(&state), NotificationLevel::AllMessages);
    set(&mut state, 3, 3, 3);
    assert_eq!(
        level(&state),
        NotificationLevel::OnlyMentions,
        "the guild's default"
    );
}

#[test]
fn collapsed_is_ignored() {
    let mut state = state_with("");
    let before = state.guild_settings(Some(Snowflake::new(G1)));
    let mut update = ready_value()["user_guild_settings"]["entries"][0].clone();
    update["channel_overrides"][0]["collapsed"] = false.into();

    let events = apply(&mut state, "USER_GUILD_SETTINGS_UPDATE", &update);

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(state.guild_settings(Some(Snowflake::new(G1))), before);
}

#[test]
fn a_guild_update_changes_the_default_level() {
    let mut state = state_with("");

    let events = apply(
        &mut state,
        "GUILD_UPDATE",
        &json!({"id": G3.to_string(), "default_message_notifications": 0}),
    );

    assert_eq!(events, [format!("GuildUpdated({G3})")]);
    assert_eq!(
        state.notification_level(Snowflake::new(G3_WELCOME)),
        NotificationLevel::AllMessages
    );
}

#[test]
fn notification_settings_update_replaces_the_flags() {
    let mut state = state_with("");
    assert_eq!(state.settings().notification_flags, 16);

    let events = apply(
        &mut state,
        "NOTIFICATION_SETTINGS_UPDATE",
        &json!({"flags": 48}),
    );

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(state.settings().notification_flags, 48);
}

// The settings proto

#[test]
fn a_partial_proto_update_replaces_only_its_fields() {
    let mut state = state_with(&proto(&[folder(7, &[G1, G3, G4])]));

    let versions = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&versions_only(), true),
    );
    assert!(versions.is_empty(), "{versions:?}");
    assert_eq!(items(&state), [format!("folder 7: {G1}, {G3}, {G4}")]);

    let folders = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&proto(&[plain(&[G4, G3, G1])]), true),
    );
    assert_eq!(folders, ["ServerListChanged"]);
    assert_eq!(items(&state), [s(G4), s(G3), s(G1)]);
}

#[test]
fn a_full_proto_update_without_folders_clears_them() {
    let mut state = state_with(&proto(&[plain(&[G1, G3, G4])]));

    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&versions_only(), false),
    );

    assert_eq!(events, ["ServerListChanged"]);
    assert_eq!(items(&state), [s(G4), s(G3), s(G1)]);
}

#[test]
fn a_proto_that_doesnt_decode_keeps_the_folders() {
    let mut state = ready_with(&proto(&[folder(7, &[G1])]));

    let update = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update("not base64!", false),
    );
    assert!(update.is_empty(), "{update:?}");

    let mut ready = ready_value();
    ready["user_settings_proto"] = STANDARD.encode([0x72, 0x09]).into();
    let again = apply(&mut state, "READY", &ready);
    assert!(
        !again.contains(&"ServerListChanged".to_owned()),
        "{again:?}"
    );
    assert_eq!(items(&state), [format!("folder 7: {G1}")]);
}

#[test]
fn a_first_ready_whose_proto_doesnt_decode_uses_the_join_order() {
    let mut ready = ready_value();
    ready["user_settings_proto"] = "not base64!".into();
    let mut state = State::new();

    state.apply(dispatch("READY", &ready), &mut Vec::new());

    assert_eq!(items(&state), [s(G1)]);
}

#[test]
fn another_settings_type_changes_nothing() {
    let mut state = state_with(&proto(&[plain(&[G1, G3, G4])]));

    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &json!({"settings": {"type": 2, "proto": proto(&[plain(&[G4])])}, "partial": false}),
    );

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(items(&state), [s(G1), s(G3), s(G4)]);
}

// Events

#[test]
fn a_settings_update_with_equal_values_sends_nothing() {
    let mut state = state_with("");
    let same = ready_value()["user_guild_settings"]["entries"][0].clone();

    let events = apply(&mut state, "USER_GUILD_SETTINGS_UPDATE", &same);

    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn a_guild_settings_update_sends_one_event() {
    let mut state = state_with("");
    let mut update = ready_value()["user_guild_settings"]["entries"][0].clone();
    update["suppress_roles"] = true.into();

    let events = apply(&mut state, "USER_GUILD_SETTINGS_UPDATE", &update);

    assert_eq!(events, [format!("GuildSettingsChanged({G1})")]);
    assert!(
        state
            .guild_settings(Some(Snowflake::new(G1)))
            .suppress_roles
    );
}

#[test]
fn settings_for_dms_send_their_event() {
    let mut state = state_with("");
    let mut update = ready_value()["user_guild_settings"]["entries"][1].clone();
    update["muted"] = true.into();

    let events = apply(&mut state, "USER_GUILD_SETTINGS_UPDATE", &update);

    assert_eq!(events, ["GuildSettingsChanged(0)"]);
}

#[test]
fn a_folder_change_sends_server_list_changed() {
    let mut state = state_with(&proto(&[plain(&[G1, G3, G4])]));

    let same = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&proto(&[plain(&[G1, G3, G4])]), false),
    );
    let moved = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&proto(&[plain(&[G1]), folder(7, &[G3, G4])]), false),
    );

    assert!(same.is_empty(), "{same:?}");
    assert_eq!(moved, ["ServerListChanged"]);
}

#[test]
fn joining_and_leaving_change_the_server_list() {
    let mut state = state_with("");

    let joined = apply(
        &mut state,
        "GUILD_CREATE",
        &guild_create(
            UNKNOWN,
            300_000_000_000_000_091,
            "2026-10-10T12:00:00.000000+00:00",
        ),
    );
    let left = apply(&mut state, "GUILD_DELETE", &json!({"id": G3.to_string()}));
    let down = apply(
        &mut state,
        "GUILD_DELETE",
        &json!({"id": G4.to_string(), "unavailable": true}),
    );

    assert_eq!(
        joined,
        [
            format!("GuildAdded({UNKNOWN})"),
            "ServerListChanged".to_owned()
        ]
    );
    assert_eq!(
        left,
        [
            format!("GuildRemoved({G3})"),
            "ServerListChanged".to_owned()
        ]
    );
    assert_eq!(down.last().map(String::as_str), Some("ServerListChanged"));
    assert_eq!(items(&state), [s(UNKNOWN), s(G1)]);
}

#[test]
fn a_new_ready_reports_only_changed_settings() {
    let mut state = ready_with(&proto(&[plain(&[G1])]));
    let mut ready = ready_value();
    ready["user_settings_proto"] = proto(&[folder(7, &[G1])]).into();
    ready["user_guild_settings"]["entries"][0]["suppress_roles"] = true.into();

    let events = apply(&mut state, "READY", &ready);

    assert!(
        events.contains(&format!("GuildSettingsChanged({G1})")),
        "{events:?}"
    );
    assert!(
        !events.contains(&"GuildSettingsChanged(0)".to_owned()),
        "{events:?}"
    );
    assert!(
        events.contains(&"ServerListChanged".to_owned()),
        "{events:?}"
    );
    assert_eq!(events.last().map(String::as_str), Some("Ready"));
}

#[test]
fn an_equal_ready_reports_no_settings() {
    let mut state = ready_with(&proto(&[plain(&[G1])]));
    let mut ready = ready_value();
    ready["user_settings_proto"] = proto(&[plain(&[G1])]).into();

    let events = apply(&mut state, "READY", &ready);

    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("GuildSettingsChanged")),
        "{events:?}"
    );
    assert!(
        !events.contains(&"ServerListChanged".to_owned()),
        "{events:?}"
    );
}

#[test]
fn the_store_reads_settings() {
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(dispatch("READY", &ready_value()));

    assert!(
        store
            .guild_settings(Some(Snowflake::new(G1)))
            .suppress_everyone
    );
    assert_eq!(
        *store.guild_settings(Some(Snowflake::new(UNKNOWN))),
        GuildSettings::default()
    );
    assert!(
        store.channel_muted(Snowflake::new(DM)),
        "muted until unmuted"
    );
    assert!(
        !store.channel_muted(Snowflake::new(GENERAL)),
        "the mute ended"
    );
    assert!(!store.channel_muted(Snowflake::new(UNKNOWN)));
    assert_eq!(
        store.notification_level(Snowflake::new(GENERAL)),
        NotificationLevel::Nothing
    );
}

#[test]
fn the_store_reads_mutes_for_guilds_and_batches_of_channels() {
    let store = Store::new(DEFAULT_LIMITS);
    store.apply(dispatch("READY", &ready_value()));

    let ids = [GENERAL, DM, UNKNOWN, GROUP].map(Snowflake::new);
    assert_eq!(store.muted_channels(&ids), [Snowflake::new(DM)]);
    assert!(!store.guild_muted(Some(Snowflake::new(G1))));

    let mut update = guild_settings_update(G1, json!([]));
    update["muted"] = true.into();
    store.apply(dispatch("USER_GUILD_SETTINGS_UPDATE", &update));

    assert!(store.guild_muted(Some(Snowflake::new(G1))));
    assert!(!store.guild_muted(None));
}

// Display settings (text_and_images)

fn bool_value(number: u64, value: bool) -> Vec<u8> {
    field(
        number,
        Wire::Bytes(field(1, Wire::Varint(u64::from(value)))),
    )
}

fn u32_value(number: u64, value: u32) -> Vec<u8> {
    field(
        number,
        Wire::Bytes(field(1, Wire::Varint(u64::from(value)))),
    )
}

fn string_value(number: u64, value: &str) -> Vec<u8> {
    field(
        number,
        Wire::Bytes(field(1, Wire::Bytes(value.as_bytes().to_vec()))),
    )
}

fn redactions(number: u64, guilds: u64, friend_dm: u64, non_friend_dm: u64) -> Vec<u8> {
    field(
        number,
        Wire::Bytes(
            [
                field(1, Wire::Varint(guilds)),
                field(2, Wire::Varint(friend_dm)),
                field(3, Wire::Varint(non_friend_dm)),
            ]
            .concat(),
        ),
    )
}

fn display_proto(fields: &[Vec<u8>]) -> String {
    STANDARD.encode(field(6, Wire::Bytes(fields.concat())))
}

fn quiet_display() -> String {
    display_proto(&[
        string_value(4, "ALWAYS"),
        bool_value(7, true),
        bool_value(9, false),
        bool_value(10, false),
        bool_value(11, false),
        bool_value(12, false),
        bool_value(14, false),
        u32_value(15, 2),
        u32_value(19, 2),
        redactions(29, 1, 2, 3),
        redactions(32, 3, 3, 3),
        redactions(35, 2, 0, 1),
    ])
}

#[test]
fn display_settings_take_discords_defaults_without_the_field() {
    let state = ready_with(&versions_only());

    assert_eq!(
        state.display_settings(),
        DisplaySettings {
            render_spoilers: SpoilerSetting::OnClick,
            view_image_descriptions: false,
            inline_attachment_media: true,
            inline_embed_media: true,
            gif_auto_play: true,
            render_embeds: true,
            animate_emoji: true,
            animate_stickers: StickerAnimation::Always,
            explicit_content_filter: ExplicitContentFilter::NonFriends,
            explicit_content: ExplicitContentSettings::default(),
            gore: ExplicitContentSettings::default(),
            self_harm: ExplicitContentSettings::default(),
        }
    );
    assert_eq!(state.display_settings(), DisplaySettings::default());
    assert_eq!(
        ready_with("").display_settings(),
        DisplaySettings::default()
    );
}

#[test]
fn ready_display_settings_are_read() {
    let state = ready_with(&quiet_display());

    assert_eq!(
        state.display_settings(),
        DisplaySettings {
            render_spoilers: SpoilerSetting::Always,
            view_image_descriptions: true,
            inline_attachment_media: false,
            inline_embed_media: false,
            gif_auto_play: false,
            render_embeds: false,
            animate_emoji: false,
            animate_stickers: StickerAnimation::Never,
            explicit_content_filter: ExplicitContentFilter::All,
            explicit_content: ExplicitContentSettings {
                guilds: Redaction::Show,
                friend_dm: Redaction::Blur,
                non_friend_dm: Redaction::Block,
            },
            gore: ExplicitContentSettings {
                guilds: Redaction::Block,
                friend_dm: Redaction::Block,
                non_friend_dm: Redaction::Block,
            },
            self_harm: ExplicitContentSettings {
                guilds: Redaction::Blur,
                friend_dm: Redaction::Unset,
                non_friend_dm: Redaction::Show,
            },
        }
    );
}

#[test]
fn unknown_display_values_take_the_defaults() {
    let state = ready_with(&display_proto(&[
        string_value(4, "SOMETIMES"),
        u32_value(15, 7),
        u32_value(19, 9),
        redactions(29, 8, 1, 0),
    ]));

    let display = state.display_settings();
    assert_eq!(display.render_spoilers, SpoilerSetting::OnClick);
    assert_eq!(display.animate_stickers, StickerAnimation::Always);
    assert_eq!(
        display.explicit_content_filter,
        ExplicitContentFilter::NonFriends
    );
    assert_eq!(display.explicit_content.guilds, Redaction::Unset);
    assert_eq!(display.explicit_content.friend_dm, Redaction::Show);
}

#[test]
fn a_proto_update_changes_the_display_settings_and_says_so() {
    let mut state = ready_with(&versions_only());

    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&display_proto(&[bool_value(11, false)]), true),
    );
    assert_eq!(events, ["DisplaySettingsChanged"]);
    assert!(!state.display_settings().gif_auto_play);
    assert!(state.display_settings().animate_emoji);

    // The same values again: nothing changed.
    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&display_proto(&[bool_value(11, false)]), true),
    );
    assert_eq!(events, Vec::<String>::new());

    // A partial update without the field keeps the settings.
    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&versions_only(), true),
    );
    assert_eq!(events, Vec::<String>::new());
    assert!(!state.display_settings().gif_auto_play);

    // A full update without the field resets them to the defaults.
    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update(&versions_only(), false),
    );
    assert_eq!(events, ["DisplaySettingsChanged"]);
    assert_eq!(state.display_settings(), DisplaySettings::default());
}

#[test]
fn an_unreadable_proto_keeps_the_display_settings() {
    let mut state = ready_with(&quiet_display());

    let events = apply(
        &mut state,
        "USER_SETTINGS_PROTO_UPDATE",
        &proto_update("not base64!", false),
    );

    assert_eq!(events, Vec::<String>::new());
    assert!(!state.display_settings().gif_auto_play);
}

#[tokio::test]
async fn a_new_ready_reports_changed_display_settings() {
    let store = Store::new(DEFAULT_LIMITS);
    let mut ready = ready_value();
    ready["user_settings_proto"] = quiet_display().into();
    store.apply(dispatch("READY", &ready));
    let subscription = store.subscribe();

    store.apply(dispatch("READY", &ready_value()));
    let names: Vec<String> = subscription
        .next_batch(256)
        .await
        .iter()
        .map(describe)
        .filter(|name| name == "DisplaySettingsChanged")
        .collect();

    assert_eq!(names, ["DisplaySettingsChanged"]);
    assert_eq!(store.display_settings(), DisplaySettings::default());
}

#[test]
fn the_store_reads_display_settings() {
    let store = Store::new(DEFAULT_LIMITS);
    let mut ready = ready_value();
    ready["user_settings_proto"] = quiet_display().into();
    store.apply(dispatch("READY", &ready));

    assert_eq!(
        store.display_settings().animate_stickers,
        StickerAnimation::Never
    );
}
